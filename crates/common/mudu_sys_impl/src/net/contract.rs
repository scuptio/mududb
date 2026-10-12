use mudu::common::result::RS;
use std::net::{SocketAddr, TcpListener, TcpStream};

/// Blocking TCP networking contract over `std::net` types.
pub trait SyncNet: Send + Sync {
    /// Binds a TCP listener to `addr`.
    fn bind_tcp(&self, addr: SocketAddr) -> RS<TcpListener>;
    /// Opens a TCP connection to `addr`.
    fn connect_tcp(&self, addr: SocketAddr) -> RS<TcpStream>;
}
