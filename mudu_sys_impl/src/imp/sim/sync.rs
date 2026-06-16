use crate::io::fd::RawFd;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

pub mod blocking {
    use crate::io::fd::RawFd;
    pub use crate::sync::blocking::{
        ChannelReceiver, ChannelSender, ChannelSyncSender, channel, sync_channel,
    };
    use mudu::common::result::RS;

    pub fn eventfd() -> RS<RawFd> {
        super::Sync::eventfd()
    }

    pub fn notify_eventfd(fd: RawFd) -> RS<()> {
        super::Sync::notify_eventfd(fd)
    }

    pub fn read_eventfd(fd: RawFd) -> RS<u64> {
        super::Sync::read_eventfd(fd)
    }

    pub fn close_fd(fd: RawFd) -> RS<()> {
        super::Sync::close_fd(fd)
    }
}

pub mod async_ {
    pub use crate::sync::async_::{
        AMutex, AMutexGuard, ANotified, ANotify, ARwLock, ARwLockReadGuard, ARwLockWriteGuard,
        FMutex, FMutexGuard, Notify, StopRx, StopTx, Wait, create_notify_wait, stop_channel,
    };
    pub use crate::sync::async_::{Notifier, Waiter, notify_wait};
}

pub struct Sync;

impl Sync {
    pub fn eventfd() -> RS<RawFd> {
        Err(m_error!(EC::NotImplemented, "[sim] Sync::eventfd"))
    }

    pub fn notify_eventfd(_fd: RawFd) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] Sync::notify_eventfd"))
    }

    pub fn read_eventfd(_fd: RawFd) -> RS<u64> {
        Err(m_error!(EC::NotImplemented, "[sim] Sync::read_eventfd"))
    }

    pub fn close_fd(_fd: RawFd) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] Sync::close_fd"))
    }
}
