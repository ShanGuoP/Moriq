//! Diagnose a real installation using the same checksum and resume path as the desktop.
//! Run with an isolated APPDATA to avoid modifying an existing model installation.
use moriq_ai::{DownloadControl, install_with_mirror, installed_path};
/// 按指定下载来源安装模型，验证正式校验和与续传路径。
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let model = args
        .next()
        .ok_or("usage: install_model MODEL mirror|official")?;
    let use_mirror = match args.next().as_deref() {
        Some("mirror") => true,
        Some("official") => false,
        _ => return Err("source must be mirror or official".into()),
    };
    let control = DownloadControl::default();
    install_with_mirror(&model, &control, use_mirror)?;
    println!(
        "installed {}: {} bytes; {}",
        model,
        control
            .downloaded
            .load(std::sync::atomic::Ordering::Relaxed),
        installed_path(&model)?.display()
    );
    Ok(())
}
