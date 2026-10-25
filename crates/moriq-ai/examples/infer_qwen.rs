//! Exercise the controlled llama server with the installed Qwen weights.
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let task = args
        .next()
        .ok_or("usage: infer_qwen classify|describe IMAGE [PROFILE_JSON]")?;
    let image = args
        .next()
        .ok_or("usage: infer_qwen classify|describe IMAGE [PROFILE_JSON]")?;
    let profile = args
        .next()
        .map(|path| {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            serde_json::from_slice::<moriq_domain::AiProfile>(&bytes).map_err(|e| e.to_string())
        })
        .transpose()?
        .unwrap_or_default();
    let result = moriq_ai::llama_infer_profile(
        &task,
        std::path::Path::new(&image),
        &std::sync::atomic::AtomicBool::new(false),
        &profile,
    );
    if let Err(error) = moriq_ai::unload_llama() {
        eprintln!("unload deferred: {error}");
    }
    println!(
        "{}",
        serde_json::to_string(&result?).map_err(|e| e.to_string())?
    );
    Ok(())
}
