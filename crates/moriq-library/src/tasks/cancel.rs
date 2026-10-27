//! Scoped cancellation follows a blocking media job without global cross-job state.
#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
/// 验证取消可中断分块复制，并不会影响之后任务。
fn cancellation_interrupts_chunked_copy_and_does_not_leak_to_the_next_job() {
    struct Reader(Arc<AtomicBool>);
    impl Read for Reader {
        /// 提供可控字节读取与取消触发，供分块复制测试使用。
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.0.store(true, Ordering::Release);
            bytes[..4].copy_from_slice(b"data");
            Ok(4)
        }
    }
    let flag = Arc::new(AtomicBool::new(false));
    let mut output = Vec::new();
    {
        let _scope = Scope::enter(flag.clone());
        assert_eq!(
            copy(&mut Reader(flag), &mut output).unwrap_err().kind(),
            std::io::ErrorKind::Interrupted
        );
    }
    assert_eq!(output, b"data");
    assert!(check().is_ok());
}
use super::*;
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
thread_local! { static CURRENT: RefCell<Option<Arc<AtomicBool>>>=const {RefCell::new(None)}; }
pub(crate) struct Scope(Option<Arc<AtomicBool>>);
impl Scope {
    /// 为当前线程安装取消标志，并保存进入前的取消上下文。
    pub fn enter(flag: Arc<AtomicBool>) -> Self {
        Self(CURRENT.with(|current| current.replace(Some(flag))))
    }
}
impl Drop for Scope {
    /// 恢复进入作用域之前的取消标志，防止取消状态泄漏到下一项任务。
    fn drop(&mut self) {
        CURRENT.with(|current| current.replace(self.0.take()));
    }
}
/// 读取当前线程作用域中的取消状态。
pub(crate) fn stopped() -> bool {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    })
}
/// 将当前取消状态转换为可传播的资源库错误。
pub(crate) fn check() -> Result<()> {
    if stopped() {
        Err(Error::Invalid("后台任务已停止".into()))
    } else {
        Ok(())
    }
}
/// 分块复制字节流，并在块间检查取消，避免长复制无法及时停止。
pub(crate) fn copy(input: &mut impl Read, output: &mut impl Write) -> std::io::Result<u64> {
    let mut total = 0;
    let mut buffer = [0u8; 65536];
    loop {
        if stopped() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "后台任务已停止",
            ));
        }
        let n = input.read(&mut buffer)?;
        if n == 0 {
            return Ok(total);
        }
        output.write_all(&buffer[..n])?;
        total += n as u64;
    }
}
