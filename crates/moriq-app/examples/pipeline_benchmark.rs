//! Service pipeline benchmark against explicitly supplied, generated fixtures.
use moriq_app::{ImportControl, Service};
use std::{path::PathBuf, sync::atomic::Ordering, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().ok_or("input directory")?);
    let output = PathBuf::from(args.next().ok_or("output parent")?);
    std::fs::create_dir_all(&output)?;
    let workers = args
        .next()
        .and_then(|v| v.to_str()?.parse().ok())
        .unwrap_or(4);
    tokio::runtime::Runtime::new()?.block_on(async move {
        let service = Service::start()?;
        let opened = service.create(output, "pipeline-benchmark".into()).await?;
        let memory_mib = args.next().and_then(|v|v.to_str()?.parse().ok()).unwrap_or(1024);
        let control = ImportControl { workers, memory_mib, ..Default::default() };
        let start = Instant::now();
        let result = service.import_sources(opened.info.id, vec![input], None, control.clone()).await;
        let count=control.discovered.load(Ordering::Relaxed);
        println!("workers={workers} memory_mib={memory_mib} peak={} largest_mib={} count={count} imported={} duplicates={} failed={} total={:.3}s",control.peak.load(Ordering::Relaxed),control.largest_mib.load(Ordering::Relaxed),result.imported,result.duplicates,result.failed,start.elapsed().as_secs_f64());
        for failure in &result.failures { println!("{failure}"); }
        if result.failed != 0 { return Err("import failed".into()); }
        let page = service.page(opened.info.id, Default::default(), 0, 64).await?;
        if page.total != result.imported as u64 { return Err("row count mismatch".into()); }
        service.close().await?;
        Ok::<(),Box<dyn std::error::Error>>(())
    })
}
