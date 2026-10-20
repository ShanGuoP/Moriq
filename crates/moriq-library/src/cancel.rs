//! Scoped cancellation follows a blocking media job without global cross-job state.
#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
fn cancellation_interrupts_chunked_copy_and_does_not_leak_to_the_next_job() {
    struct Reader(Arc<AtomicBool>);
    impl Read for Reader {
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
    pub fn enter(flag: Arc<AtomicBool>) -> Self {
        Self(CURRENT.with(|current| current.replace(Some(flag))))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|current| current.replace(self.0.take()));
    }
}
pub(crate) fn stopped() -> bool {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    })
}
pub(crate) fn check() -> Result<()> {
    if stopped() {
        Err(Error::Invalid("后台任务已停止".into()))
    } else {
        Ok(())
    }
}
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
