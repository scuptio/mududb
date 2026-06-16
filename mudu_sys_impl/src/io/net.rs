use crate::io::fd::RawFd;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;

#[cfg(target_os = "linux")]
use crate::io::iouring::SockAddrBuf;

pub fn set_tcp_nodelay(fd: RawFd) -> RS<()> {
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
            "set tcp nodelay on raw fd error",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn socket_addr_to_storage(addr: SocketAddr) -> RS<SockAddrBuf> {
    match addr {
        SocketAddr::V4(v4) => {
            let mut storage = zeroed_sockaddr_storage();
            let raw = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: v4.port().to_be(),
                sin_addr: libc::in_addr {
                    s_addr: u32::from_be_bytes(v4.ip().octets()).to_be(),
                },
                sin_zero: [0; 8],
            };
            unsafe {
                std::ptr::write(
                    (&mut storage) as *mut rliburing::sockaddr_storage as *mut libc::sockaddr_in,
                    raw,
                );
            }
            Ok(SockAddrBuf::from_raw(
                storage,
                std::mem::size_of::<libc::sockaddr_in>() as u32,
            ))
        }
        SocketAddr::V6(v6) => {
            let mut storage = zeroed_sockaddr_storage();
            let raw = libc::sockaddr_in6 {
                sin6_family: libc::AF_INET6 as libc::sa_family_t,
                sin6_port: v6.port().to_be(),
                sin6_flowinfo: v6.flowinfo(),
                sin6_addr: libc::in6_addr {
                    s6_addr: v6.ip().octets(),
                },
                sin6_scope_id: v6.scope_id(),
            };
            unsafe {
                std::ptr::write(
                    (&mut storage) as *mut rliburing::sockaddr_storage as *mut libc::sockaddr_in6,
                    raw,
                );
            }
            Ok(SockAddrBuf::from_raw(
                storage,
                std::mem::size_of::<libc::sockaddr_in6>() as u32,
            ))
        }
    }
}

#[cfg(target_os = "linux")]
pub fn sockaddr_to_socket_addr(addr: &SockAddrBuf) -> RS<SocketAddr> {
    match addr.raw().ss_family as i32 {
        libc::AF_INET => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in>() {
                return Err(m_error!(EC::NetErr, "short sockaddr_in length"));
            }
            let raw = unsafe {
                &*(addr.raw() as *const rliburing::sockaddr_storage as *const libc::sockaddr_in)
            };
            let ip = std::net::Ipv4Addr::from(u32::from_be(raw.sin_addr.s_addr).to_be_bytes());
            Ok(SocketAddr::from((ip, u16::from_be(raw.sin_port))))
        }
        libc::AF_INET6 => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in6>() {
                return Err(m_error!(EC::NetErr, "short sockaddr_in6 length"));
            }
            let raw = unsafe {
                &*(addr.raw() as *const rliburing::sockaddr_storage as *const libc::sockaddr_in6)
            };
            let ip = std::net::Ipv6Addr::from(raw.sin6_addr.s6_addr);
            Ok(SocketAddr::from((ip, u16::from_be(raw.sin6_port))))
        }
        family => Err(m_error!(
            EC::NetErr,
            format!("unsupported socket family {}", family)
        )),
    }
}

#[cfg(target_os = "linux")]
fn zeroed_sockaddr_storage() -> rliburing::sockaddr_storage {
    unsafe { std::mem::zeroed() }
}
