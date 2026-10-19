//! Generated video acceptance; no external media or user library is modified.
use moriq_domain::{ExportOptions, MediaKind, Query};
use moriq_library::{Library, video};
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parent = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("pass test output directory")?,
    );
    fs::create_dir_all(&parent)?;
    let source = parent.join("测试视频 sample.mp4");
    let status = std::process::Command::new(video::tool("ffmpeg")?)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "4",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
        ])
        .arg(&source)
        .status()?;
    if !status.success() {
        return Err("fixture generation failed".into());
    }
    let original = fs::read(&source)?;
    let metadata = video::probe(&source)?;
    assert_eq!((metadata.width, metadata.height), (320, 180));
    assert!(metadata.has_audio);
    assert!(metadata.duration_ms >= 3900);
    let mut lib = Library::create(&parent, "视频验证")?;
    lib.import(&source, None)?;
    let query = Query {
        media_kind: MediaKind::Video,
        ..Default::default()
    };
    let page = lib.page(&query, 0, 4)?;
    assert_eq!(page.total, 1);
    let asset = &page.assets[0];
    assert!(!lib.import(&source, None)?);
    let preview = lib.preview(asset.id)?;
    assert!(preview.width > 0);
    let details = lib.details_plan(asset.id)?.read();
    assert!(details.exif.iter().any(|(k, _)| k == "时长"));
    let exported = moriq_library::export::run(
        lib.export_plan(&[asset.id])?,
        &parent,
        ExportOptions::default(),
    )?;
    assert_eq!(fs::read(exported.join("测试视频 sample.mp4"))?, original);
    let root = lib.root().to_path_buf();
    drop(lib);
    let mut reopened = Library::open(&root)?;
    assert_eq!(reopened.page(&query, 0, 4)?.total, 1);
    assert_eq!(fs::read(&source)?, original);
    fs::write(
        parent.join("report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","library":root,"metadata":metadata,"preview":[preview.width,preview.height],"source_unchanged":true,"duplicate_skipped":true,"reopen":true}),
        )?,
    )?;
    println!("{}", root.display());
    Ok(())
}
