use mudu::common::result::RS;
use std::net::SocketAddr;

use crate::io::fd::RawFd;

pub trait SysNet: Send + Sync {
    fn create_tcp_listener_fd(&self, listen_addr: SocketAddr, backlog: i32) -> RS<RawFd>;
    fn set_tcp_nodelay(&self, fd: RawFd) -> RS<()>;
}
