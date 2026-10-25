//! Isolated APPDATA is required. Exercise the actual registry and application writer.
use moriq_app::Service;
fn main() -> Result<(), String> {
    tokio::runtime::Runtime::new()
        .map_err(|e| e.to_string())?
        .block_on(run())
}
async fn run() -> Result<(), String> {
    if std::env::var("MORIQ_ISOLATED_LIFECYCLE_PROBE").as_deref() != Ok("1") {
        return Err("isolated settings required".into());
    }
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let service = Service::start().map_err(|e| e.to_string())?;
    let mut template = moriq_domain::AiProfile::default();
    template.categories.push(moriq_domain::AiCategory {
        id: "theme_reusable".into(),
        kind: moriq_domain::AiCategoryKind::Theme,
        label: "复用验证".into(),
        definition: "可复用的分类说明".into(),
        enabled: true,
    });
    service.save_category_template(template).await?;
    let a = service
        .create(dir.path().into(), "删除验证库".into())
        .await?;
    assert!(
        service
            .organizer(a.info.id)
            .await?
            .ai_profile
            .categories
            .iter()
            .any(|c| c.id == "theme_reusable")
    );
    service
        .remember(a.info.name.clone(), a.root.clone())
        .await?;
    let external = dir.path().join("外部原片.txt");
    std::fs::write(&external, b"preserve").map_err(|e| e.to_string())?;
    let plan = service.library_removal_plan(a.root.clone()).await?;
    assert!(
        service
            .remove_library(a.root.clone(), Some(plan))
            .await
            .is_err()
    );
    service.close_library(a.info.id).await?;
    let settings = service.remove_library(a.root.clone(), None).await?;
    assert!(settings.recent.is_empty());
    assert!(a.root.exists());
    let reopened = service.open(a.root.clone()).await?;
    service.remember(reopened.info.name, reopened.root).await?;
    let b = service.create(dir.path().into(), "保留库".into()).await?;
    service.remember(b.info.name, b.root.clone()).await?;
    let plan = service.library_removal_plan(a.root.clone()).await?;
    let prefs = service.remove_library(a.root.clone(), Some(plan)).await?;
    assert!(!a.root.exists());
    assert!(b.root.exists());
    assert_eq!(prefs.recent.len(), 1);
    assert_eq!(
        std::fs::read(external).map_err(|e| e.to_string())?,
        b"preserve"
    );
    assert_eq!(service.info(b.info.id).await?.id, b.info.id);
    service.close_library(b.info.id).await?;
    println!(
        "{}",
        serde_json::json!({"new_library_inherits_template":true,"forget_preserved_files":true,"reopen":true,"permanent_deleted_only_target":true,"other_active_library_preserved":true,"external_original_preserved":true})
    );
    Ok(())
}
