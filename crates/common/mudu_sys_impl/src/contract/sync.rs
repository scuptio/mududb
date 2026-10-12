use mudu::common::result::RS;

use crate::imp::io::fd::RawFd;

/// Blocking synchronization service: Linux `eventfd`-style counters and raw
/// file-descriptor cleanup.
///
/// The `eventfd` operations are only implemented on Linux; on other
/// platforms they return a `NotImplemented` error.
pub trait SysSync: Send + Sync {
    /// Creates a new `eventfd` counter (initial value 0, close-on-exec) and
    /// returns its raw file descriptor.
    fn eventfd(&self) -> RS<RawFd>;
    /// Adds 1 to the counter of the eventfd referred to by `fd`, waking any
    /// blocked readers.
    fn notify_eventfd(&self, fd: RawFd) -> RS<()>;
    /// Reads and resets the counter of the eventfd referred to by `fd`,
    /// returning the accumulated value.
    fn read_eventfd(&self, fd: RawFd) -> RS<u64>;
    /// Closes the raw file descriptor `fd`.
    fn close_fd(&self, fd: RawFd) -> RS<()>;
}
