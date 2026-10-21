//! Measure cold decode, disk-cache reopen and memory-cache reuse in isolated libraries.
use moriq_app::{ImportControl, Service};
use moriq_domain::Query;
use std::{
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
fn main() -> Result<(), String> {
    tokio::runtime::Runtime::new()
        .map_err(|e| e.to_string())?
        .block_on(async {
            let mut args = std::env::args().skip(1);
            let base = PathBuf::from(
                args.next()
                    .ok_or("usage: preview_benchmark OUTPUT_DIRECTORY [RAW_FILE]")?,
            );
            let root = base.join(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_millis()
                    .to_string(),
            );
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let jpeg = root.join("24mp.jpg");
            image::RgbImage::from_fn(4016, 6016, |x, y| {
                image::Rgb([
                    ((x / 7 + y / 9) % 256) as u8,
                    ((x / 11 + y / 5) % 256) as u8,
                    ((x + y / 3) % 256) as u8,
                ])
            })
            .save(&jpeg)
            .map_err(|e| e.to_string())?;
            let mut sources = vec![("JPEG-24MP", jpeg)];
            if let Some(raw) = args.next() {
                sources.push(("RAW", PathBuf::from(raw)));
            }
            let mut records = Vec::new();
            for (label, source) in sources {
                let service = Service::start().map_err(|e| e.to_string())?;
                let library = service.create(root.clone(), label.into()).await?;
                let report = service
                    .import_sources(
                        library.info.id,
                        vec![source],
                        None,
                        ImportControl::default(),
                    )
                    .await;
                if report.imported != 1 {
                    return Err(format!("import failed: {:?}", report.failures));
                }
                let asset = service
                    .page(library.info.id, Query::default(), 0, 1)
                    .await?
                    .tiles
                    .remove(0)
                    .asset;
                let start = Instant::now();
                let cold = service
                    .preview_version(library.info.id, asset.id, asset.version_id)
                    .await?;
                let cold_ms = start.elapsed().as_secs_f64() * 1000.;
                service.close().await?;
                service.open(library.root.clone()).await?;
                let start = Instant::now();
                let disk = service
                    .preview_version(library.info.id, asset.id, asset.version_id)
                    .await?;
                let disk_ms = start.elapsed().as_secs_f64() * 1000.;
                let start = Instant::now();
                let memory = service
                    .preview_version(library.info.id, asset.id, asset.version_id)
                    .await?;
                let memory_ms = start.elapsed().as_secs_f64() * 1000.;
                if cold.image.rgba != disk.image.rgba || disk.image.rgba != memory.image.rgba {
                    return Err("cache pixels differ".into());
                }
                records.push(serde_json::json!({
                    "media": label, "library": library.root,
                    "source_dimensions": [asset.width, asset.height],
                    "preview_dimensions": [cold.image.width, cold.image.height],
                    "cold_ms": cold_ms, "disk_ms": disk_ms, "memory_ms": memory_ms,
                    "pixels_identical": true,
                }));
                service.close().await?;
            }
            let json = serde_json::to_string_pretty(&records).map_err(|e| e.to_string())?;
            std::fs::write(base.join("preview-benchmark.json"), &json)
                .map_err(|e| e.to_string())?;
            println!("{json}");
            Ok(())
        })
}
