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
        #[cfg(windows)]
        let target = shell_target(target);
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

#[cfg(windows)]
fn shell_target(value: OsString) -> OsString {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let wide: Vec<u16> = value.encode_wide().collect();
    let prefix: Vec<u16> = "\\\\?\\".encode_utf16().collect();
    let unc: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();
    if wide.starts_with(&unc) {
        let mut normal: Vec<u16> = "\\\\".encode_utf16().collect();
        normal.extend_from_slice(&wide[unc.len()..]);
        OsString::from_wide(&normal)
    } else if wide.starts_with(&prefix) && wide.get(5) == Some(&(b':' as u16)) {
        OsString::from_wide(&wide[prefix.len()..])
    } else {
        value
    }
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn explorer_paths_remove_verbatim_prefix_without_changing_unicode_spaces_or_urls() {
        for (input, expected) in [
            (
                r"\\?\C:\用户 目录\MORIQ\models",
                r"C:\用户 目录\MORIQ\models",
            ),
            (r"\\?\UNC\server\share\模型", r"\\server\share\模型"),
            ("https://example.com/project", "https://example.com/project"),
        ] {
            assert_eq!(shell_target(input.into()), OsString::from(expected));
        }
    }
}
