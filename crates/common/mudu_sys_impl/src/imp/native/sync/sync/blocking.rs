use crate::imp::io::fd::RawFd;
use mudu::common::result::RS;

pub fn eventfd() -> RS<RawFd> {
    crate::imp::sync::Sync::eventfd()
}

pub fn notify_eventfd(fd: RawFd) -> RS<()> {
    crate::imp::sync::Sync::notify_eventfd(fd)
}

pub fn read_eventfd(fd: RawFd) -> RS<u64> {
    crate::imp::sync::Sync::read_eventfd(fd)
}

pub fn close_fd(fd: RawFd) -> RS<()> {
    crate::imp::sync::Sync::close_fd(fd)
}
