#![allow(missing_docs)]
use crate::imp::io::fd::RawFd;
use mudu::common::result::RS;
#[cfg(target_os = "linux")]
use mudu::error::others::io_error_with_message;
#[cfg(not(target_os = "linux"))]
use mudu::error::ErrorCode;
#[cfg(not(target_os = "linux"))]
use mudu::mudu_error;

pub mod async_;
#[allow(clippy::module_inception)]
pub mod sync;

pub use async_::*;
pub use sync::blocking;
pub use sync::channel;
pub use sync::std_mutex;
pub use sync::std_rwlock;
pub use sync::unbounded_channel;

pub struct Sync;

impl Sync {
    #[cfg(target_os = "linux")]
    pub fn eventfd() -> RS<RawFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if fd < 0 {
            return Err(io_error_with_message(
                std::io::Error::last_os_error(),
                "create eventfd error",
            ));
        }
        Ok(fd)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn eventfd() -> RS<RawFd> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "eventfd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    pub fn notify_eventfd(fd: RawFd) -> RS<()> {
        let value: u64 = 1;
        let rc = unsafe {
            libc::write(
                fd,
                &value as *const u64 as *const libc::c_void,
                std::mem::size_of::<u64>(),
            )
        };
        if rc as usize != std::mem::size_of::<u64>() {
            return Err(io_error_with_message(
                std::io::Error::last_os_error(),
                "write eventfd error",
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn notify_eventfd(_fd: RawFd) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "notify_eventfd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    pub fn read_eventfd(fd: RawFd) -> RS<u64> {
        let mut value = 0u64;
        let rc = unsafe {
            libc::read(
                fd,
                (&mut value) as *mut u64 as *mut libc::c_void,
                std::mem::size_of::<u64>(),
            )
        };
        if rc as usize != std::mem::size_of::<u64>() {
            return Err(io_error_with_message(
                std::io::Error::last_os_error(),
                "read eventfd error",
            ));
        }
        Ok(value)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn read_eventfd(_fd: RawFd) -> RS<u64> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "read_eventfd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    pub fn close_fd(fd: RawFd) -> RS<()> {
        let rc = unsafe { libc::close(fd) };
        if rc != 0 {
            return Err(io_error_with_message(
                std::io::Error::last_os_error(),
                "close fd error",
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn close_fd(_fd: RawFd) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "close_fd is only available on Linux"
        ))
    }
}

#[derive(Default)]
pub struct SysSync;

impl SysSync {
    pub fn new() -> Self {
        Self
    }

    pub fn eventfd(&self) -> RS<RawFd> {
        blocking::eventfd()
    }

    pub fn notify_eventfd(&self, fd: RawFd) -> RS<()> {
        blocking::notify_eventfd(fd)
    }

    pub fn read_eventfd(&self, fd: RawFd) -> RS<u64> {
        blocking::read_eventfd(fd)
    }

    pub fn close_fd(&self, fd: RawFd) -> RS<()> {
        blocking::close_fd(fd)
    }

    pub fn mutex<T>(&self, value: T) -> std_mutex::SMutex<T> {
        std_mutex::SMutex::new(value)
    }

    pub fn rwlock<T>(&self, value: T) -> std_rwlock::SRwLock<T> {
        std_rwlock::SRwLock::new(value)
    }

    pub fn channel<T>(
        &self,
    ) -> (
        unbounded_channel::ChannelSender<T>,
        unbounded_channel::SyncReceiver<T>,
    ) {
        unbounded_channel::unbounded_channel()
    }

    pub fn sync_channel<T>(
        &self,
        bound: usize,
    ) -> (channel::SyncSender<T>, unbounded_channel::SyncReceiver<T>) {
        channel::sync_channel(bound)
    }

    pub fn async_mutex<T>(&self, value: T) -> async_::AMutex<T> {
        async_::AMutex::new(value)
    }

    pub fn async_rwlock<T>(&self, value: T) -> async_::ARwLock<T> {
        async_::ARwLock::new(value)
    }

    pub fn async_notify(&self) -> async_::ANotify {
        async_::ANotify::new()
    }

    pub fn stop_channel(&self) -> (async_::StopTx, async_::StopRx) {
        async_::stop_channel()
    }

    pub fn async_notify_wait(&self) -> (async_::Notifier, async_::Waiter) {
        async_::notify_wait()
    }

    pub fn futures_mutex<T>(&self, value: T) -> async_::FMutex<T> {
        async_::FMutex::new(value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    #[cfg(not(target_os = "linux"))]
    use mudu::error::ErrorCode;

    #[cfg(target_os = "linux")]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn eventfd_create_and_close() {
        let fd = Sync::eventfd().unwrap();
        assert!(fd >= 0);
        Sync::close_fd(fd).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn eventfd_notify_then_read_returns_one() {
        let fd = Sync::eventfd().unwrap();
        Sync::notify_eventfd(fd).unwrap();
        assert_eq!(Sync::read_eventfd(fd).unwrap(), 1);
        Sync::close_fd(fd).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn eventfd_three_notifies_then_read_returns_three() {
        let fd = Sync::eventfd().unwrap();
        for _ in 0..3 {
            Sync::notify_eventfd(fd).unwrap();
        }
        assert_eq!(Sync::read_eventfd(fd).unwrap(), 3);
        Sync::close_fd(fd).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn read_eventfd_after_close_fails() {
        let fd = Sync::eventfd().unwrap();
        Sync::close_fd(fd).unwrap();
        assert!(Sync::read_eventfd(fd).is_err());
    }

    #[cfg(target_os = "linux")]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn close_fd_invalid_fails() {
        assert!(Sync::close_fd(-1).is_err());
    }

    #[cfg(not(target_os = "linux"))]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn eventfd_returns_not_implemented_on_non_linux() {
        let err = Sync::eventfd().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[cfg(not(target_os = "linux"))]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn notify_eventfd_returns_not_implemented_on_non_linux() {
        let err = Sync::notify_eventfd(0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[cfg(not(target_os = "linux"))]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn read_eventfd_returns_not_implemented_on_non_linux() {
        let err = Sync::read_eventfd(0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[cfg(not(target_os = "linux"))]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn close_fd_returns_not_implemented_on_non_linux() {
        let err = Sync::close_fd(0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn sys_sync_mutex_lock_unlock_roundtrip() {
        let sys = SysSync::new();
        let mutex = sys.mutex(0);
        {
            let mut guard = mutex.lock().unwrap();
            *guard += 1;
        }
        let guard = mutex.lock().unwrap();
        assert_eq!(*guard, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn sys_sync_channel_send_recv_roundtrip() {
        let sys = SysSync::new();
        let (tx, rx) = sys.channel::<i32>();
        tx.send(42).unwrap();
        assert_eq!(rx.recv().unwrap(), 42);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test(flavor = "current_thread")]
    async fn sys_sync_async_mutex_lock_unlock_roundtrip() {
        let sys = SysSync::new();
        let mutex = sys.async_mutex(0);
        {
            let mut guard = mutex.lock().await;
            *guard += 1;
        }
        let guard = mutex.lock().await;
        assert_eq!(*guard, 1);
    }
}
