mod async_tokio_listener;
mod async_tokio_net;
mod async_tokio_stream;
mod host;
mod test_tokio_net;
mod tokio_tcp_listener;
mod tokio_tcp_stream;

use mudu::common::result::RS;
use std::net::SocketAddr;

pub use tokio_tcp_listener::TokioTcpListener;
pub use tokio_tcp_stream::TokioTcpStream;

use crate::imp::net::async_tokio::host::ToAddrs;

pub(crate) use async_tokio_net::TokioNet;

/// Binds a Tokio TCP listener to `addr`.
pub async fn bind_tcp(addr: SocketAddr) -> RS<TokioTcpListener> {
    TokioTcpListener::bind(addr).await
}

/// Opens a Tokio TCP connection to `addr`.
pub async fn connect_tcp(addr: SocketAddr) -> RS<TokioTcpStream> {
    TokioTcpStream::connect(addr).await
}

/// Resolves `addr` (host name or address string) into socket addresses.
pub async fn lookup_host<A: ToAddrs>(addr: A) -> RS<Vec<SocketAddr>> {
    host::lookup_host(addr).await
}

/// Converts a blocking standard-library listener into a Tokio listener.
pub fn listener_from_std(listener: std::net::TcpListener) -> RS<TokioTcpListener> {
    TokioTcpListener::from_std(listener)
}

/// Converts a blocking standard-library stream into a Tokio stream.
pub fn stream_from_std(stream: std::net::TcpStream) -> RS<TokioTcpStream> {
    TokioTcpStream::from_std(stream)
}
