use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

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
        return Err(m_error!(
            EC::NetErr,
            "set tcp nodelay on raw fd error",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
