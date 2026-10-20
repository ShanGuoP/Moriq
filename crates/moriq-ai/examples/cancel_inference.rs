//! Explicit installed-model cancellation probe, without any library writes.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let model = args.next().ok_or("usage: cancel_inference MODEL IMAGE")?;
    let image = args.next().ok_or("pass a licensed or generated image")?;
    moriq_ai::set_keep_loaded(true)?;
    let request = serde_json::json!({"image":image});
    moriq_ai::infer(&model, request.clone())?;
    let flag = Arc::new(AtomicBool::new(false));
    let signal = flag.clone();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        signal.store(true, Ordering::Release);
    });
    let started = std::time::Instant::now();
    let result = moriq_ai::infer_controlled(&model, request, &flag);
    timer.join().map_err(|_| "cancellation timer failed")?;
    moriq_ai::unload()?;
    let error = result
        .err()
        .ok_or("inference completed before cancellation; use a slower model")?;
    if !error.contains("已停止") {
        return Err(error);
    }
    println!(
        "{}",
        serde_json::json!({"cancelled":true,"elapsed_ms":started.elapsed().as_millis(),"model":model})
    );
    Ok(())
}
