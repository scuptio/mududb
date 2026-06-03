use crate::api::sync::SysSync;
use crate::io::fd::RawFd;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

pub struct LinuxSync;

impl SysSync for LinuxSync {
    #[cfg(target_os = "linux")]
    fn eventfd(&self) -> RS<RawFd> {
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
    fn eventfd(&self) -> RS<RawFd> {
        Err(m_error!(
            EC::NotImplemented,
            "eventfd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    fn notify_eventfd(&self, fd: RawFd) -> RS<()> {
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
    fn notify_eventfd(&self, _fd: RawFd) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "notify_eventfd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    fn read_eventfd(&self, fd: RawFd) -> RS<u64> {
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
    fn read_eventfd(&self, _fd: RawFd) -> RS<u64> {
        Err(m_error!(
            EC::NotImplemented,
            "read_eventfd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    fn close_fd(&self, fd: RawFd) -> RS<()> {
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
    fn close_fd(&self, _fd: RawFd) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "close_fd is only available on Linux"
        ))
    }
}
