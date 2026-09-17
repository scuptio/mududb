use mudu::common::result::RS;
use std::net::{SocketAddr, TcpListener, TcpStream};

pub trait SyncNet: Send + Sync {
    fn bind_tcp(&self, addr: SocketAddr) -> RS<TcpListener>;
    fn connect_tcp(&self, addr: SocketAddr) -> RS<TcpStream>;
}
