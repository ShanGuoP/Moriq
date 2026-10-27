//! Temporary video fixture builder: creates an isolated demo library with licensed photos.
//! Runs under target/, not in the shipping app.
use moriq_app::{ImportControl, Service};
use moriq_domain::ImportFile;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let root = PathBuf::from(r"D:\AI\vibe\remotion\moriq\capture-data\moriq-demo");
        let images = PathBuf::from(r"D:\AI\vibe\remotion\moriq\public\images");
        let service = Service::start()?;
        let opened = match service.create(root.clone(), "光影素材库".into()).await {
            Ok(o) => o,
            Err(_) => service.open(root.join("光影素材库.moriq")).await?,
        };
        let project = service
            .project(opened.info.id, "Aurora 人像练习".into())
            .await?;
        let paths: Vec<PathBuf> = std::fs::read_dir(&images)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| matches!(p.extension().and_then(|s| s.to_str()), Some("jpg")))
            .collect();
        let files: Vec<ImportFile> = paths
            .iter()
            .map(|p| {
                Ok(ImportFile {
                    path: p.clone(),
                    relative: p.file_name().ok_or("Image path has no file name")?.into(),
                    project_name: None,
                })
            })
            .collect::<Result<_, &str>>()?;
        let report = service
            .import(
                opened.info.id,
                files,
                Some(project),
                ImportControl::default(),
            )
            .await;
        println!("imported={} failed={}", report.imported, report.failed);
        service.close().await?;
        Ok(())
    })
}
