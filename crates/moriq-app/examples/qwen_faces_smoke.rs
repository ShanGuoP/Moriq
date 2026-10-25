//! Real Qwen face detection and grouping against temporary public sample media.
use moriq_domain::{JobKind, JobState, Query};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), String> {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("Expected two source images and a report directory".into());
    }
    tokio::runtime::Runtime::new()
        .map_err(|e| e.to_string())?
        .block_on(run(&args))
}
async fn run(args: &[PathBuf]) -> Result<(), String> {
    std::fs::create_dir_all(&args[2]).map_err(|e| e.to_string())?;
    let dir = tempfile::tempdir_in(&args[2]).map_err(|e| e.to_string())?;
    let first = image::open(&args[0]).map_err(|e| e.to_string())?;
    let other = image::open(&args[1]).map_err(|e| e.to_string())?;
    let mut lib = moriq_library::Library::create(dir.path(), "Qwen人脸实际验证")
        .map_err(|e| e.to_string())?;
    for (name, image) in [
        ("original.png", first.clone()),
        (
            "resized.png",
            first.resize(384, 384, image::imageops::FilterType::Triangle),
        ),
        ("other.png", other),
    ] {
        let path = dir.path().join(name);
        image.save(&path).map_err(|e| e.to_string())?;
        lib.import(&path, None).map_err(|e| e.to_string())?;
    }
    let root = lib.root().to_owned();
    drop(lib);
    let service = moriq_app::Service::start().map_err(|e| e.to_string())?;
    let owner = service.open(root).await?.info.id;
    moriq_ai::set_keep_loaded(true)?;
    let started = Instant::now();
    service
        .queue_media_job(owner, JobKind::QwenFaces)
        .await
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let jobs = service.background_jobs(owner).await?;
        if jobs.iter().all(|j| j.state == JobState::Completed) {
            break;
        }
        if Instant::now() > deadline {
            return Err(format!("Face inference timed out: {jobs:?}"));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let page = service.page(owner, Query::default(), 0, 10).await?;
    let mut evidence = Vec::new();
    for tile in page.tiles {
        let details = service.details(owner, tile.asset.id).await?;
        let faces = details
            .faces
            .iter()
            .map(|f| serde_json::json!({"person":f.person,"box":f.bounds}))
            .collect::<Vec<_>>();
        evidence.push(serde_json::json!({"name":tile.asset.name,"faces":faces}));
    }
    let people = service.organizer(owner).await?.people;
    let groups = people
        .iter()
        .map(|p| serde_json::json!({"id":p.id,"count":p.count}))
        .collect::<Vec<_>>();
    let jobs = service.background_jobs(owner).await?;
    let report = serde_json::json!({"seconds":started.elapsed().as_secs_f64(),"fixture":dir.path(),"images":evidence,"people":groups,"jobs":format!("{jobs:?}")});
    std::fs::create_dir_all(&args[2]).map_err(|e| e.to_string())?;
    std::fs::write(
        args[2].join("qwen-faces-report.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    service.close().await?;
    moriq_ai::set_keep_loaded(false)?;
    let _evidence = dir.keep();
    if jobs.iter().any(|j| j.failed > 0)
        || people.len() != 2
        || !people.iter().any(|p| p.count == 2)
    {
        return Err(format!("Unexpected grouping: {report}"));
    }
    println!(
        "Qwen detected three faces, grouped two versions of one person together, and kept the other person separate in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
