use mudu::common::result::RS;

use crate::io::fd::RawFd;

pub trait SysSync: Send + Sync {
    fn eventfd(&self) -> RS<RawFd>;
    fn notify_eventfd(&self, fd: RawFd) -> RS<()>;
    fn read_eventfd(&self, fd: RawFd) -> RS<u64>;
    fn close_fd(&self, fd: RawFd) -> RS<()>;
}
