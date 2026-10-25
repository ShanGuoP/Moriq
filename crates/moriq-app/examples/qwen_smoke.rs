//! Exercise the real scheduler with generated media, never a user's library.
use moriq_app::Service;
use moriq_domain::{AiCategory, AiCategoryKind, AiEdit, JobKind, JobState, LibraryId, Query};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

async fn analyze(service: &Service, owner: LibraryId, total: usize) -> Result<(), String> {
    for kind in [JobKind::QwenClassification, JobKind::QwenDescription] {
        service
            .queue_media_job(owner, kind)
            .await
            .map_err(|e| e.to_string())?;
    }
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let jobs = service.background_jobs(owner).await?;
        if jobs.len() == total && jobs.iter().all(|job| job.state == JobState::Completed) {
            if jobs.iter().any(|job| job.failed != 0) {
                return Err(format!("Qwen task failed: {jobs:?}"));
            }
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!("Qwen task timeout: {jobs:?}"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn run(image: PathBuf) -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut library = moriq_library::Library::create(directory.path(), "qwen-content-smoke")
        .map_err(|e| e.to_string())?;
    library.import(&image, None).map_err(|e| e.to_string())?;
    let root = library.root().to_path_buf();
    let asset = library
        .page(&Query::default(), 0, 1)
        .map_err(|e| e.to_string())?
        .assets
        .remove(0);
    drop(library);
    let service = Service::start().map_err(|e| e.to_string())?;
    let owner = service.open(root.clone()).await?.info.id;
    let mut profile = service.organizer(owner).await?.ai_profile;
    for category in &mut profile.categories {
        if category.kind == AiCategoryKind::Theme {
            category.enabled = false;
        }
    }
    profile.max_themes = 1;
    profile.categories.push(AiCategory {id:"theme_house_scene".into(),kind:AiCategoryKind::Theme,label:"房屋插画".into(),definition:"画面主体是一座卡通风格的小房子，图中可有树木、太阳、山丘。只要清楚看到房子就选入，不要求写实。".into(),enabled:true});
    service.save_ai_profile(owner, profile).await?;
    analyze(&service, owner, 2).await?;
    let details = service.details(owner, asset.id).await?;
    if !details.themes.contains(&"theme_house_scene".into()) {
        return Err("Custom theme definition did not reach effective classification".into());
    }
    let mut profile = service.organizer(owner).await?.ai_profile;
    profile
        .categories
        .last_mut()
        .ok_or("Custom category missing")?
        .label = "小屋场景".into();
    let expected_profile = service.save_ai_profile(owner, profile).await?;
    let groups = service.organizer(owner).await?.ai_groups;
    let group = groups
        .iter()
        .find(|g| g.id == "theme_house_scene")
        .ok_or("Custom group missing")?;
    if group.label != "小屋场景" || group.count != 1 || group.cover != Some(asset.version_id) {
        return Err("Custom group rename/count/cover sync failed".into());
    }
    if details.medium.is_empty() || details.ai_caption.is_empty() {
        return Err("Qwen results missing from details".into());
    }
    let search: String = details.ai_caption.chars().take(2).collect();
    let found = service
        .page(
            owner,
            Query {
                text: search,
                ..Default::default()
            },
            0,
            10,
        )
        .await?
        .total;
    if found != 1 {
        return Err("Short Qwen description search failed".into());
    }
    let filter = Query {
        medium: details.medium.clone(),
        themes: details.themes.clone(),
        ..Default::default()
    };
    if service.page(owner, filter, 0, 10).await?.total != 1 {
        return Err("Independent medium/theme filters failed".into());
    }
    for tag in &details.ai_tags {
        let filter = Query {
            tag: tag.clone(),
            ..Default::default()
        };
        if service.page(owner, filter, 0, 10).await?.total != 1 {
            return Err("Exact AI tag filtering failed".into());
        }
    }
    let rejected = details.ai_tags.first().cloned();
    if let Some(tag) = &rejected {
        service
            .edit_ai_content(
                owner,
                asset.id,
                asset.version_id,
                AiEdit::RejectTag(tag.clone()),
            )
            .await?;
    }
    let manual = "人工描述保留验证".to_string();
    service
        .edit_ai_content(
            owner,
            asset.id,
            asset.version_id,
            AiEdit::Description(Some(manual.clone())),
        )
        .await?;
    analyze(&service, owner, 4).await?;
    service.close().await?;
    service.open(root).await?;
    let restored = service.details(owner, asset.id).await?;
    if service.organizer(owner).await?.ai_profile != expected_profile {
        return Err("Custom profile lost after reopening".into());
    }
    if restored.ai_caption != manual || !restored.manual_description {
        return Err("Manual description overwritten by rerun/reopen".into());
    }
    if rejected
        .as_ref()
        .is_some_and(|tag| restored.ai_tags.contains(tag))
    {
        return Err("Rejected tag reappeared".into());
    }
    if service
        .page(
            owner,
            Query {
                text: manual,
                ..Default::default()
            },
            0,
            10,
        )
        .await?
        .total
        != 1
    {
        return Err("Manual description search failed".into());
    }
    service.close().await?;
    let report = serde_json::json!({
        "model":"qwen35-4b", "tasks_completed":4, "medium":details.medium, "themes":details.themes,
        "description":details.ai_caption, "search_matches":found, "short_search":true,
        "medium_theme_filters":true, "exact_ai_tag_filters":true, "ai_tag_count":details.ai_tags.len(),
        "rejected_tag":rejected, "manual_override_survived_rerun":true, "reopen_preserved":true
        ,"custom_theme":"theme_house_scene","custom_profile_durable":true,"renamed_group_synced":true
    });
    println!("{report}");
    Ok(())
}

fn main() -> Result<(), String> {
    let image = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("pass a generated test image")?;
    tokio::runtime::Runtime::new()
        .map_err(|e| e.to_string())?
        .block_on(run(image))
}
