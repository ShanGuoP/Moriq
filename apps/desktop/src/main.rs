#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() {
    if let Err(error) = run() {
        rfd::MessageDialog::new()
            .set_title("Moriq Space 无法启动")
            .set_description(error.to_string())
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let diagnostic_logging = moriq_ui::diagnostic_logging_enabled();
    let log_root = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .map(|path| path.join("MORIQ/logs"))
    } else {
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|path| path.join(".local/state/moriq"))
    };
    let writer: Box<dyn std::io::Write + Send> =
        match log_root.filter(|_| diagnostic_logging).and_then(|root| {
            std::fs::create_dir_all(&root).ok().and_then(|()| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(root.join("desktop.log"))
                    .ok()
            })
        }) {
            Some(file) => Box::new(file),
            None => Box::new(std::io::stderr()),
        };
    tracing_subscriber::fmt()
        .with_env_filter(if diagnostic_logging {
            "moriq=info,wgpu=warn"
        } else {
            "off"
        })
        .with_target(false)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(writer))
        .init();
    let mut launch = moriq_ui::LaunchOptions::default();
    let icon =
        image::load_from_memory(include_bytes!(concat!(env!("OUT_DIR"), "/moriq.png")))?.to_rgba8();
    launch.window_icon = Some(iced::window::icon::from_rgba(icon.into_raw(), 256, 256)?);
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--library")=>launch.library=Some(arguments.next().ok_or("--library requires a folder")?.into()),
            Some("--import")=>launch.import_paths.push(arguments.next().ok_or("--import requires a file or folder")?.into()),
            #[cfg(feature = "capture")]
            Some("--panel")=>launch.panel=Some(arguments.next().ok_or("--panel requires a panel name")?.into_string().map_err(|_|"invalid panel name")?),
            #[cfg(feature = "capture")]
            Some("--capture")=>launch.capture=Some(arguments.next().ok_or("--capture requires a PNG path")?.into()),
            #[cfg(feature = "capture")]
            Some("--screen")=>launch.screen=match arguments.next().and_then(|s|s.into_string().ok()).as_deref(){
Some("assets")=>1,
Some("models")=>2,
Some("overview")=>0,
_=>return Err("--screen expects overview, assets or models".into())}
,

            #[cfg(feature = "capture")]
            Some("--exit-after-capture")=>launch.exit_after_capture=true,
            #[cfg(feature = "capture")]
            Some("--select-first")=>launch.select_first=true,
            _=>return Err("Unknown argument. Use --library PATH or --import PATH. Diagnostic flags require the capture feature.".into()),
        }
    }
    moriq_ui::run(launch)?;
    Ok(())
}
