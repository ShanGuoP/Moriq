//! Native YuNet/SFace CPU inference, without a Python or CUDA dependency.
use crate::Result;
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub const REVISION: &str = "yunet2023mar-sface2021dec-cpu-v1";
static WORKER: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();
struct Worker {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    _log: tempfile::NamedTempFile,
}
struct WorkerLease<'a>(std::sync::MutexGuard<'a, Option<Worker>>);
impl std::ops::Deref for WorkerLease<'_> {
    type Target = Option<Worker>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for WorkerLease<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for WorkerLease<'_> {
    fn drop(&mut self) {
        if crate::llama::on_demand_idle() {
            *self.0 = None;
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn location() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let root = exe.parent().ok_or("程序目录不可用")?.join("ai/faces");
    #[cfg(debug_assertions)]
    let root = if root.join("face-worker.exe").is_file() {
        root
    } else {
        PathBuf::from(env!("MORIQ_DEV_FACE_ROOT"))
    };
    Ok(root)
}
pub fn model_bytes() -> u64 {
    location()
        .map(|root| {
            ["yunet.onnx", "sface.onnx"]
                .iter()
                .filter_map(|name| std::fs::metadata(root.join(name)).ok().map(|m| m.len()))
                .sum()
        })
        .unwrap_or(0)
}
pub fn available() -> bool {
    location().is_ok_and(|root| {
        [
            "face-worker.exe",
            "opencv_world4120.dll",
            "yunet.onnx",
            "sface.onnx",
        ]
        .iter()
        .all(|name| root.join(name).is_file())
    })
}
pub fn ready() -> Result<PathBuf> {
    let root = location()?;
    crate::verify_runtime(
        &root,
        include_str!(concat!(env!("OUT_DIR"), "/face-checksums.json")),
        &[],
    )
}
pub fn unload() -> Result<()> {
    match WORKER.get_or_init(|| Mutex::new(None)).try_lock() {
        Ok(mut worker) => *worker = None,
        Err(_) => return Err("CPU 人脸模型正在使用，请停止分析后再试".into()),
    }
    Ok(())
}
fn start() -> Result<Worker> {
    let root = ready()?;
    let log = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut command = crate::runtime::guarded_command(&root.join("face-worker.exe"))?;
    command
        .arg(root.join("yunet.onnx"))
        .arg(root.join("sface.onnx"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log.reopen().map_err(|e| e.to_string())?));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("无法启动 CPU 人脸运行器：{e}"))?;
    let input = child.stdin.take().ok_or("人脸输入通道缺失")?;
    let output = child.stdout.take().ok_or("人脸输出通道缺失")?;
    Ok(Worker {
        child,
        input,
        output,
        _log: log,
    })
}
/// 复用小型 CPU 进程；取消时终止其进程树并拒绝迟到回执。
pub fn infer(image: &Path, cancel: &AtomicBool) -> Result<Value> {
    let mutex = WORKER.get_or_init(|| Mutex::new(None));
    let slot = loop {
        if cancel.load(Ordering::Acquire) {
            return Err("人脸整理已停止".into());
        }
        match mutex.try_lock() {
            Ok(slot) => break slot,
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(_) => return Err("人脸运行器状态异常".into()),
        }
    };
    let mut slot = WorkerLease(slot);
    if slot.is_none() {
        *slot = Some(start()?);
    }
    let bytes = std::fs::read(image).map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() > 16 * 1024 * 1024 {
        return Err("人脸输入图片超出预算".into());
    }
    let worker = slot.as_mut().ok_or("人脸运行器未就绪")?;
    let result = std::thread::scope(|threads| {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let input = &mut worker.input;
        let output = &mut worker.output;
        threads.spawn(move || {
            let result = (|| -> Result<Vec<u8>> {
                input
                    .write_all(&(bytes.len() as u32).to_le_bytes())
                    .map_err(|e| e.to_string())?;
                input.write_all(&bytes).map_err(|e| e.to_string())?;
                input.flush().map_err(|e| e.to_string())?;
                let mut length = [0; 4];
                output.read_exact(&mut length).map_err(|e| e.to_string())?;
                let length = u32::from_le_bytes(length) as usize;
                if length == 0 || length > 256 * 1024 {
                    return Err("人脸回执超出预算".into());
                }
                let mut result = vec![0; length];
                output.read_exact(&mut result).map_err(|e| e.to_string())?;
                Ok(result)
            })();
            let _ = tx.send(result);
        });
        let started = Instant::now();
        loop {
            if cancel.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(30) {
                let _ = worker.child.kill();
                let _ = worker.child.wait();
                return Err("人脸整理已停止或超时".into());
            }
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(result) => return result,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("人脸推理通道中断".into()),
            }
        }
    });
    if result.is_err() {
        *slot = None;
    }
    let value: Value = serde_json::from_slice(&result?).map_err(|e| e.to_string())?;
    let faces = value["faces"]
        .as_array()
        .filter(|faces| faces.len() <= 64)
        .ok_or("人脸回执格式无效")?;
    for face in faces {
        if !face["vector"].as_array().is_some_and(|v| {
            v.len() == 128 && v.iter().all(|v| v.as_f64().is_some_and(|v| v.is_finite()))
        }) {
            return Err("人脸特征回执无效".into());
        }
    }
    if cancel.load(Ordering::Acquire) {
        return Err("人脸整理已停止".into());
    }
    Ok(value)
}

#[cfg(all(test, windows))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the bundled CPU runtime; explicitly checked during packaging"]
    /// 验证真实 CPU 进程的按需释放、批次复用及错误路径释放。
    fn bundled_cpu_runtime_obeys_residency_and_batch_lifetime() {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                let _ = crate::set_keep_loaded(false);
                let _ = crate::unload();
            }
        }
        let _reset = Reset;
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("blank.png");
        image::RgbImage::from_pixel(128, 128, image::Rgb([10, 20, 30]))
            .save(&image)
            .unwrap();
        let cancel = AtomicBool::new(false);
        let worker = WORKER.get_or_init(|| Mutex::new(None));

        crate::set_keep_loaded(false).unwrap();
        assert_eq!(
            infer(&image, &cancel).unwrap()["faces"],
            serde_json::json!([])
        );
        assert!(worker.lock().unwrap().is_none());

        crate::set_keep_loaded(true).unwrap();
        infer(&image, &cancel).unwrap();
        assert!(worker.lock().unwrap().is_some());
        crate::set_keep_loaded(false).unwrap();
        assert!(worker.lock().unwrap().is_none());

        let batch = crate::batch_lease();
        infer(&image, &cancel).unwrap();
        let pid = worker.lock().unwrap().as_ref().unwrap().child.id();
        infer(&image, &cancel).unwrap();
        assert_eq!(worker.lock().unwrap().as_ref().unwrap().child.id(), pid);
        crate::set_keep_loaded(false).unwrap();
        assert!(worker.lock().unwrap().is_some());
        drop(batch);
        assert!(worker.lock().unwrap().is_none());

        crate::set_keep_loaded(true).unwrap();
        infer(&image, &cancel).unwrap();
        {
            let _lease = WorkerLease(worker.lock().unwrap());
            assert!(unload().is_err());
            crate::set_keep_loaded(false).unwrap();
        }
        assert!(worker.lock().unwrap().is_none());
        assert!(infer(&dir.path().join("missing.png"), &cancel).is_err());
        assert!(worker.lock().unwrap().is_none());
    }
}
