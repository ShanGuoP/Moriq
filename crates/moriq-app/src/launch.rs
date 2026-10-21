//! Desktop shell integration always runs outside the catalog writer.
use crate::Result;
use std::{ffi::OsString, process::Command};
pub(super) async fn open(target: OsString, reveal: bool) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        let mut command = Command::new("explorer.exe");
        #[cfg(target_os = "macos")]
        let mut command = Command::new("open");
        #[cfg(not(any(windows, target_os = "macos")))]
        let mut command = Command::new("xdg-open");
        if reveal {
            #[cfg(windows)]
            {
                let mut argument = OsString::from("/select,");
                argument.push(target);
                command.arg(argument);
            }
            #[cfg(target_os = "macos")]
            command.arg("-R").arg(target);
            #[cfg(not(any(windows, target_os = "macos")))]
            command.arg(
                std::path::Path::new(&target)
                    .parent()
                    .ok_or("文件没有上级目录")?,
            );
        } else {
            command.arg(target);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("无法打开系统应用：{e}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
