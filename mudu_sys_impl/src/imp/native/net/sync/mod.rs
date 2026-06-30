#![allow(missing_docs)]
mod tcp_listener;
mod tcp_stream;

use mudu::common::result::RS;
use std::net::SocketAddr;

pub use tcp_listener::TcpListener as TcpListenerSync;
pub use tcp_stream::TcpStream as TcpStreamSync;

pub fn bind_tcp(addr: SocketAddr) -> RS<TcpListenerSync> {
    TcpListenerSync::bind(addr)
}

pub fn connect_tcp(addr: SocketAddr) -> RS<TcpStreamSync> {
    TcpStreamSync::connect(addr)
}
