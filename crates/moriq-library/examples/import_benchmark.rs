//! Explicit generated-media benchmark; never run on user folders.
use moriq_library::Library;
use std::{path::PathBuf, time::Instant};
/// 使用显式指定的测试素材测量导入耗时，并输出统计结果。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().ok_or("input directory")?);
    let output = PathBuf::from(args.next().ok_or("output parent")?);
    let mut paths = std::fs::read_dir(input)?
        .map(|r| r.map(|r| r.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let mut lib = Library::create(&output, "import-benchmark")?;
    let total = Instant::now();
    for path in &paths {
        let start = Instant::now();
        let result = lib.import(path, None)?;
        let phases = lib.import_metrics();
        println!(
            "phases copy={:.2}ms hash={:.2}ms decode={:.2}ms thumbnail={:.2}ms commit={:.2}ms",
            phases.copy_ms, phases.hash_ms, phases.decode_ms, phases.thumbnail_ms, phases.commit_ms
        );
        println!(
            "{} imported={} {:.3}s",
            path.file_name().unwrap_or_default().to_string_lossy(),
            result,
            start.elapsed().as_secs_f64()
        );
    }
    println!(
        "count={} total={:.3}s",
        paths.len(),
        total.elapsed().as_secs_f64()
    );
    Ok(())
}
