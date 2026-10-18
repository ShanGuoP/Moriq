//! Explicitly supplied licensed/generated fixture only; all outputs stay under the supplied root.
use moriq_domain::{ExportFormat, ExportOptions, Query};
use moriq_library::Library;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::PathBuf};
fn hash(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("pass fixture path")?);
    let parent = PathBuf::from(args.next().ok_or("pass existing output root")?);
    let before = hash(&source)?;
    let mut lib = Library::create(&parent, "RAW验证")?;
    let project = lib.create_project("RAW许可样本")?;
    let start = std::time::Instant::now();
    let result = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        lib.import(&source, Some(project))?;
        let asset = lib.page(&Query::default(), 0, 1)?.assets.remove(0);
        if lib
            .page(
                &Query {
                    media_kind: moriq_domain::MediaKind::Raw,
                    ..Default::default()
                },
                0,
                1,
            )?
            .total
            != 1
            || lib.organizer()?.raw_count != 1
        {
            return Err("RAW format query mismatch".into());
        }
        lib.save_collection(
            "RAW 集合",
            &Query {
                media_kind: moriq_domain::MediaKind::Raw,
                ..Default::default()
            },
        )?;
        let preview = lib.preview(asset.id)?;
        let png = parent.join("preview.png");
        image::save_buffer(
            &png,
            &preview.rgba,
            preview.width,
            preview.height,
            image::ColorType::Rgba8,
        )?;
        let exported = moriq_library::export::run(
            lib.export_plan(&[asset.id])?,
            &parent,
            ExportOptions {
                format: ExportFormat::Png,
                max_edge: 2048,
                ..Default::default()
            },
        )?;
        let root = lib.root().to_path_buf();
        let id = lib.id();
        drop(lib);
        let reopened = Library::open(&root)?;
        if reopened.id() != id || reopened.info()?.count != 1 {
            return Err("reopen identity mismatch".into());
        }
        Ok(
            serde_json::json!({"status":"passed","format":asset.format,"width":asset.width,"height":asset.height,"preview":[preview.width,preview.height],"export":exported,"reopen":true}),
        )
    })();
    let unchanged = before == hash(&source)?;
    let report = match result {
        Ok(value) => value,
        Err(e) => serde_json::json!({"status":"failed","error":e.to_string()}),
    };
    let report = serde_json::json!({"result":report,"source_sha256":before,"source_unchanged":unchanged,"elapsed_seconds":start.elapsed().as_secs_f64()});
    fs::write(
        parent.join("media-probe.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    println!("{report}");
    if !unchanged || report["result"]["status"] != "passed" {
        return Err("media probe failed; see report".into());
    }
    Ok(())
}
