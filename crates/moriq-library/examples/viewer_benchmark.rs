//! Measure visible original-resolution tiles in an isolated copy of a library.
use moriq_domain::{ImageRegion, Query};
use moriq_library::Library;
use std::{fs, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("pass isolated library path")?);
    let output = PathBuf::from(args.next().ok_or("pass output JSON path")?);
    let mut lib = Library::open(&root)?;
    let asset = lib.page(&Query::default(), 0, 1)?.assets.remove(0);
    lib.cache_plan().trim(0)?;
    let start = Instant::now();
    let preview = lib.media_source(asset.id)?.preview()?;
    let preview_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let _disk = lib.media_source(asset.id)?.preview()?;
    let disk_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut runs = Vec::new();
    for _ in 0..2 {
        let start = Instant::now();
        let session = lib.media_source(asset.id)?.regions()?;
        let session_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut first_ms = 0.;
        let tiles = Instant::now();
        let x0 = session.width.saturating_sub(1536) / 2;
        let y0 = session.height.saturating_sub(1024) / 2;
        for row in 0..2 {
            for col in 0..3 {
                let x = (x0 + col * 512).min(session.width - 1);
                let y = (y0 + row * 512).min(session.height - 1);
                let tile = session.tile(ImageRegion {
                    x,
                    y,
                    width: 512.min(session.width - x),
                    height: 512.min(session.height - y),
                    level: 1,
                })?;
                assert_eq!(
                    tile.rgba.len(),
                    tile.width as usize * tile.height as usize * 4
                );
                if row == 0 && col == 0 {
                    first_ms = start.elapsed().as_secs_f64() * 1000.;
                }
            }
        }
        runs.push(serde_json::json!({"session_ms":session_ms,"first_original_tile_ms":first_ms,
            "six_tiles_ms":tiles.elapsed().as_secs_f64()*1000.,"total_ms":start.elapsed().as_secs_f64()*1000.}));
    }
    let report = serde_json::json!({"asset":asset.name,"dimensions":[asset.width,asset.height],
        "preview_dimensions":[preview.image.width,preview.image.height],"first_1600_ms":preview_ms,
        "disk_1600_ms":disk_ms,"runs":runs,"os_cache_cleared":false});
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(output, &json)?;
    println!("{json}");
    Ok(())
}
