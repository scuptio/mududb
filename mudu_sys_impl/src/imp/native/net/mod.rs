use mudu::common::result::RS;
use std::net::SocketAddr;

pub(crate) mod async_;
mod async_io_uring;
pub mod async_tokio;
pub mod sync;

pub struct Net;

impl Default for Net {
    fn default() -> Self {
        Self::new()
    }
}

impl Net {
    pub fn new() -> Self {
        Self
    }

    pub async fn bind_tcp(&self, addr: SocketAddr) -> RS<async_tokio::TokioTcpListener> {
        async_tokio::bind_tcp(addr).await
    }

    pub async fn connect_tcp(&self, addr: SocketAddr) -> RS<async_tokio::TokioTcpStream> {
        async_tokio::connect_tcp(addr).await
    }

    pub fn bind_tcp_sync(&self, addr: SocketAddr) -> RS<sync::TcpListenerSync> {
        sync::bind_tcp(addr)
    }

    pub fn connect_tcp_sync(&self, addr: SocketAddr) -> RS<sync::TcpStreamSync> {
        sync::connect_tcp(addr)
    }

    pub async fn lookup_host<A: tokio::net::ToSocketAddrs>(&self, addr: A) -> RS<Vec<SocketAddr>> {
        async_tokio::lookup_host(addr).await
    }

    pub fn listener_from_std(
        &self,
        listener: std::net::TcpListener,
    ) -> RS<async_tokio::TokioTcpListener> {
        async_tokio::listener_from_std(listener)
    }

    pub fn stream_from_std(&self, stream: std::net::TcpStream) -> RS<async_tokio::TokioTcpStream> {
        async_tokio::stream_from_std(stream)
    }
}
