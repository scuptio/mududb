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
    #[cfg(target_os = "linux")]
    pub fn eventfd() -> RS<RawFd> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if fd < 0 {
            return Err(m_error!(
                EC::NetErr,
                "create eventfd error",
                std::io::Error::last_os_error()
            ));
        }
        Ok(fd)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn eventfd() -> RS<RawFd> {
        Err(m_error!(
            EC::NotImplemented,
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
            return Err(m_error!(
                EC::NetErr,
                "write eventfd error",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn notify_eventfd(_fd: RawFd) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
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
            return Err(m_error!(
                EC::NetErr,
                "read eventfd error",
                std::io::Error::last_os_error()
            ));
        }
        Ok(value)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn read_eventfd(_fd: RawFd) -> RS<u64> {
        Err(m_error!(
            EC::NotImplemented,
            "read_eventfd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    pub fn close_fd(fd: RawFd) -> RS<()> {
        let rc = unsafe { libc::close(fd) };
        if rc != 0 {
            return Err(m_error!(
                EC::NetErr,
                "close fd error",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn close_fd(_fd: RawFd) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "close_fd is only available on Linux"
        ))
    }
}
