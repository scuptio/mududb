use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

pub fn set_nodelay_fd(fd: std::os::fd::RawFd) -> RS<()> {
    let flag: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_NODELAY,
            &flag as *const _ as *const libc::c_void,
            size_of_val(&flag) as libc::socklen_t,
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::set_nodelay_fd;
    use mudu::error::ErrorCode;
    use std::os::fd::AsRawFd;

    // Miri does not support setsockopt(TCP_NODELAY) on sockets.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn set_nodelay_fd_on_tcp_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let fd = listener.as_raw_fd();
        set_nodelay_fd(fd).unwrap();
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn set_nodelay_fd_invalid_fd_returns_network_error() {
        let err = set_nodelay_fd(-1).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Network);
    }
}
