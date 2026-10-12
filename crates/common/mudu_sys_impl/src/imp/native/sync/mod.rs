use crate::imp::io::fd::RawFd;
use mudu::common::result::RS;
#[cfg(target_os = "linux")]
use mudu::error::others::io_error_with_message;
#[cfg(not(target_os = "linux"))]
use mudu::error::ErrorCode;
#[cfg(not(target_os = "linux"))]
use mudu::mudu_error;

/// Async synchronization primitives (mutexes, rwlocks, notify, channels).
pub mod async_;
#[allow(clippy::module_inception)]
/// Blocking synchronization primitives over `std::sync` and libc.
pub mod sync;

pub use async_::*;
pub use sync::blocking;
pub use sync::channel;
pub use sync::std_mutex;
pub use sync::std_rwlock;
pub use sync::unbounded_channel;

/// Namespace for raw Linux `eventfd` and file-descriptor operations.
///
/// All operations are only implemented on Linux; on other platforms they
/// return a `NotImplemented` error.
pub struct Sync;

impl Sync {
    /// Creates a new `eventfd` counter (initial value 0, close-on-exec) and
    /// returns its raw file descriptor (Linux only).
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

    /// Always fails with `NotImplemented`: `eventfd` is Linux-only.
    #[cfg(not(target_os = "linux"))]
    pub fn eventfd() -> RS<RawFd> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "eventfd is only available on Linux"
        ))
    }

    /// Adds 1 to the counter of the eventfd referred to by `fd`, waking any
    /// blocked readers (Linux only).
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

    /// Always fails with `NotImplemented`: `eventfd` is Linux-only.
    #[cfg(not(target_os = "linux"))]
    pub fn notify_eventfd(_fd: RawFd) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "notify_eventfd is only available on Linux"
        ))
    }

    /// Reads and resets the counter of the eventfd referred to by `fd`,
    /// returning the accumulated value (Linux only).
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

    /// Always fails with `NotImplemented`: `eventfd` is Linux-only.
    #[cfg(not(target_os = "linux"))]
    pub fn read_eventfd(_fd: RawFd) -> RS<u64> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "read_eventfd is only available on Linux"
        ))
    }

    /// Closes the raw file descriptor `fd` (Linux only).
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

    /// Always fails with `NotImplemented`: raw-fd closing is Linux-only.
    #[cfg(not(target_os = "linux"))]
    pub fn close_fd(_fd: RawFd) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "close_fd is only available on Linux"
        ))
    }
}

/// Synchronization subsystem - native implementation.
///
/// Factory for blocking mutexes/rwlocks/channels, async
/// mutexes/rwlocks/notifiers, and Linux `eventfd` operations.
#[derive(Default)]
pub struct SysSync;

impl SysSync {
    /// Creates the synchronization subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Creates a new `eventfd` counter and returns its raw file descriptor
    /// (Linux only).
    pub fn eventfd(&self) -> RS<RawFd> {
        blocking::eventfd()
    }

    /// Adds 1 to the counter of the eventfd referred to by `fd` (Linux
    /// only).
    pub fn notify_eventfd(&self, fd: RawFd) -> RS<()> {
        blocking::notify_eventfd(fd)
    }

    /// Reads and resets the counter of the eventfd referred to by `fd`
    /// (Linux only).
    pub fn read_eventfd(&self, fd: RawFd) -> RS<u64> {
        blocking::read_eventfd(fd)
    }

    /// Closes the raw file descriptor `fd` (Linux only).
    pub fn close_fd(&self, fd: RawFd) -> RS<()> {
        blocking::close_fd(fd)
    }

    /// Creates a blocking mutex protecting `value`.
    pub fn mutex<T>(&self, value: T) -> std_mutex::SMutex<T> {
        std_mutex::SMutex::new(value)
    }

    /// Creates a blocking read-write lock protecting `value`.
    pub fn rwlock<T>(&self, value: T) -> std_rwlock::SRwLock<T> {
        std_rwlock::SRwLock::new(value)
    }

    /// Creates an unbounded blocking MPSC channel.
    pub fn channel<T>(
        &self,
    ) -> (
        unbounded_channel::ChannelSender<T>,
        unbounded_channel::SyncReceiver<T>,
    ) {
        unbounded_channel::unbounded_channel()
    }

    /// Creates a bounded blocking MPSC channel whose queue holds at most
    /// `bound` messages.
    pub fn sync_channel<T>(
        &self,
        bound: usize,
    ) -> (channel::SyncSender<T>, unbounded_channel::SyncReceiver<T>) {
        channel::sync_channel(bound)
    }

    /// Creates an async (Tokio-based) mutex protecting `value`.
    pub fn async_mutex<T>(&self, value: T) -> async_::AMutex<T> {
        async_::AMutex::new(value)
    }

    /// Creates an async (Tokio-based) read-write lock protecting `value`.
    pub fn async_rwlock<T>(&self, value: T) -> async_::ARwLock<T> {
        async_::ARwLock::new(value)
    }

    /// Creates an async single-shot-style notifier (sticky notify).
    pub fn async_notify(&self) -> async_::ANotify {
        async_::ANotify::new()
    }

    /// Creates a stop-signal channel: `stop()` on the sender flips the
    /// shared flag observed by the receiver.
    pub fn stop_channel(&self) -> (async_::StopTx, async_::StopRx) {
        async_::stop_channel()
    }

    /// Creates a one-shot notify/wait pair: the first `notify_all` wakes all
    /// current and future waiters.
    pub fn async_notify_wait(&self) -> (async_::Notifier, async_::Waiter) {
        async_::notify_wait()
    }

    /// Creates an async mutex backed by `futures::lock::Mutex` protecting
    /// `value`.
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
