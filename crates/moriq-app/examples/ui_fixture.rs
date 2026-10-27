//! Generated UI acceptance fixture, never production library data.
use moriq_domain::*;
/// 启动异步运行环境生成独立的界面验收资源库。
fn main() -> Result<(), String> {
    tokio::runtime::Runtime::new()
        .map_err(|e| e.to_string())?
        .block_on(run())
}
/// 在隔离配置下创建项目与暂停任务样本，登记为可打开的验收库。
async fn run() -> Result<(), String> {
    if std::env::var("MORIQ_ISOLATED_LIFECYCLE_PROBE").as_deref() != Ok("1") {
        return Err("isolated settings required".into());
    }
    let parent = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or("fixture parent required")?;
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let mut library =
        moriq_library::Library::create(&parent, "资源库管理验证").map_err(|e| e.to_string())?;
    if let Some(image) = std::env::args_os().nth(2) {
        library
            .import(&std::path::PathBuf::from(image), None)
            .map_err(|e| e.to_string())?;
    }
    let a = library
        .create_project("项目管理验证")
        .map_err(|e| e.to_string())?;
    library
        .create_child_project("归档", Some(a))
        .map_err(|e| e.to_string())?;
    library
        .enqueue_media_job(JobKind::QwenClassification, 0)
        .map_err(|e| e.to_string())?;
    library
        .enqueue_media_job(JobKind::QwenDescription, 0)
        .map_err(|e| e.to_string())?;
    library
        .control_model_jobs(JobCommand::Pause)
        .map_err(|e| e.to_string())?;
    let root = library.root().to_path_buf();
    drop(library);
    let service = moriq_app::Service::start().map_err(|e| e.to_string())?;
    service
        .remember("资源库管理验证".into(), root.clone())
        .await?;
    println!("{}", root.display());
    Ok(())
}
