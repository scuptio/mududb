mod tcp_listener;
mod tcp_stream;

use mudu::common::result::RS;
use std::net::SocketAddr;

pub use tcp_listener::TcpListener as TcpListenerSync;
pub use tcp_stream::TcpStream as TcpStreamSync;

/// Binds a blocking TCP listener to `addr` (with `SO_REUSEADDR` enabled).
pub fn bind_tcp(addr: SocketAddr) -> RS<TcpListenerSync> {
    TcpListenerSync::bind(addr)
}

/// Opens a blocking TCP connection to `addr` (with `TCP_NODELAY` enabled).
pub fn connect_tcp(addr: SocketAddr) -> RS<TcpStreamSync> {
    TcpStreamSync::connect(addr)
}
