use crate::api::net::SysNet;
use crate::io::fd::RawFd;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
#[cfg(target_os = "linux")]
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, IntoRawFd};

pub struct LinuxNet;

impl SysNet for LinuxNet {
    #[cfg(target_os = "linux")]
    fn create_tcp_listener_fd(&self, listen_addr: SocketAddr, backlog: i32) -> RS<RawFd> {
        let domain = if listen_addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
            .map_err(|e| m_error!(EC::NetErr, "create tcp listener socket error", e))?;
        socket
            .set_reuse_address(true)
            .map_err(|e| m_error!(EC::NetErr, "enable SO_REUSEADDR error", e))?;
        enable_reuse_port(&socket)?;
        socket
            .bind(&listen_addr.into())
            .map_err(|e| m_error!(EC::NetErr, "bind io_uring tcp listener error", e))?;
        socket
            .listen(backlog)
            .map_err(|e| m_error!(EC::NetErr, "listen io_uring tcp listener error", e))?;
        Ok(socket.into_raw_fd())
    }

    #[cfg(not(target_os = "linux"))]
    fn create_tcp_listener_fd(&self, _listen_addr: SocketAddr, _backlog: i32) -> RS<RawFd> {
        Err(m_error!(
            EC::NotImplemented,
            "create_tcp_listener_fd is only available on Linux"
        ))
    }

    #[cfg(target_os = "linux")]
    fn set_tcp_nodelay(&self, fd: RawFd) -> RS<()> {
        let flag: libc::c_int = 1;
        let rc = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_NODELAY,
                &flag as *const _ as *const libc::c_void,
                std::mem::size_of_val(&flag) as libc::socklen_t,
            )
        };
        if rc != 0 {
            return Err(m_error!(
                EC::NetErr,
                "set connection nodelay error",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    fn set_tcp_nodelay(&self, _fd: RawFd) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "set_tcp_nodelay is only available on Linux"
        ))
    }
}

#[cfg(target_os = "linux")]
fn enable_reuse_port(socket: &Socket) -> RS<()> {
    let value: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &value as *const _ as *const libc::c_void,
            std::mem::size_of_val(&value) as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(m_error!(
            EC::NetErr,
            "enable SO_REUSEPORT error",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
