#[cfg(unix)]
use super::fd::RawFd;
#[cfg(unix)]
use mudu::common::result::RS;
#[cfg(unix)]
use mudu::error::ErrorCode;
#[cfg(unix)]
use mudu::mudu_error;
#[cfg(target_os = "linux")]
use std::net::SocketAddr;

#[cfg(target_os = "linux")]
use crate::imp::native::linux::io_uring::iouring::SockAddrBuf;

/// Sets `TCP_NODELAY` (disables Nagle's algorithm) on the socket referred
/// to by raw file descriptor `fd`.
#[cfg(unix)]
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
        return Err(mudu_error!(
            ErrorCode::Network,
            "set tcp nodelay on raw fd error",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Converts a [`SocketAddr`] into a raw `sockaddr_storage` buffer suitable
/// for io_uring socket operations (Linux only).
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

/// Converts a raw `sockaddr_storage` buffer back into a [`SocketAddr`]
/// (Linux only). Supports IPv4 and IPv6; other families return an error.
#[cfg(target_os = "linux")]
pub fn sockaddr_to_socket_addr(addr: &SockAddrBuf) -> RS<SocketAddr> {
    match addr.raw().ss_family as i32 {
        libc::AF_INET => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in>() {
                return Err(mudu_error!(ErrorCode::Network, "short sockaddr_in length"));
            }
            let raw = unsafe {
                &*(addr.raw() as *const rliburing::sockaddr_storage as *const libc::sockaddr_in)
            };
            let ip = std::net::Ipv4Addr::from(u32::from_be(raw.sin_addr.s_addr).to_be_bytes());
            Ok(SocketAddr::from((ip, u16::from_be(raw.sin_port))))
        }
        libc::AF_INET6 => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in6>() {
                return Err(mudu_error!(ErrorCode::Network, "short sockaddr_in6 length"));
            }
            let raw = unsafe {
                &*(addr.raw() as *const rliburing::sockaddr_storage as *const libc::sockaddr_in6)
            };
            let ip = std::net::Ipv6Addr::from(raw.sin6_addr.s6_addr);
            Ok(SocketAddr::from((ip, u16::from_be(raw.sin6_port))))
        }
        family => Err(mudu_error!(
            ErrorCode::Network,
            format!("unsupported socket family {}", family)
        )),
    }
}

#[cfg(target_os = "linux")]
fn zeroed_sockaddr_storage() -> rliburing::sockaddr_storage {
    unsafe { std::mem::zeroed() }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn set_tcp_nodelay_on_a_real_socket() {
        use std::os::fd::AsRawFd;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        super::set_tcp_nodelay(listener.as_raw_fd()).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn socket_addr_storage_roundtrip_v4_and_v6() {
        let v4: std::net::SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let storage = super::socket_addr_to_storage(v4).unwrap();
        assert_eq!(super::sockaddr_to_socket_addr(&storage).unwrap(), v4);

        let v6: std::net::SocketAddr = "[::1]:9090".parse().unwrap();
        let storage = super::socket_addr_to_storage(v6).unwrap();
        assert_eq!(super::sockaddr_to_socket_addr(&storage).unwrap(), v6);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sockaddr_to_socket_addr_rejects_bad_family_and_short_buffers() {
        use crate::imp::native::linux::io_uring::iouring::SockAddrBuf;

        let mut storage = super::zeroed_sockaddr_storage();
        storage.ss_family = libc::AF_UNIX as libc::sa_family_t;
        let unknown = SockAddrBuf::from_raw(storage, 16);
        assert!(super::sockaddr_to_socket_addr(&unknown).is_err());

        // Right family, but the buffer is too short for the address struct.
        let mut storage = super::zeroed_sockaddr_storage();
        storage.ss_family = libc::AF_INET as libc::sa_family_t;
        let short = SockAddrBuf::from_raw(storage, 2);
        assert!(super::sockaddr_to_socket_addr(&short).is_err());

        let mut storage = super::zeroed_sockaddr_storage();
        storage.ss_family = libc::AF_INET6 as libc::sa_family_t;
        let short = SockAddrBuf::from_raw(storage, 2);
        assert!(super::sockaddr_to_socket_addr(&short).is_err());
    }
}
