use crate::imp::io::fd::RawFd;
use mudu::common::result::RS;

/// Creates a new `eventfd` counter and returns its raw file descriptor
/// (Linux only; `NotImplemented` error elsewhere).
pub fn eventfd() -> RS<RawFd> {
    crate::imp::sync::Sync::eventfd()
}

/// Adds 1 to the counter of the eventfd referred to by `fd`, waking any
/// blocked readers (Linux only).
pub fn notify_eventfd(fd: RawFd) -> RS<()> {
    crate::imp::sync::Sync::notify_eventfd(fd)
}

/// Reads and resets the counter of the eventfd referred to by `fd`,
/// returning the accumulated value (Linux only).
pub fn read_eventfd(fd: RawFd) -> RS<u64> {
    crate::imp::sync::Sync::read_eventfd(fd)
}

/// Closes the raw file descriptor `fd` (Linux only).
pub fn close_fd(fd: RawFd) -> RS<()> {
    crate::imp::sync::Sync::close_fd(fd)
}
