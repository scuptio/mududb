use crate::net::to_addrs::ToAddrs;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Async TCP listener; resolves host names before binding.
pub struct AsyncTcpListener(crate::imp::net::async_tokio::TokioTcpListener);
/// Async TCP stream implementing [`AsyncRead`] and [`AsyncWrite`].
pub struct AsyncTcpStream(crate::imp::net::async_tokio::TokioTcpStream);

impl AsyncTcpListener {
    /// Resolves `addr` and binds a listener to the first resolved address.
    pub async fn bind<A: ToAddrs>(addr: A) -> RS<Self> {
        let addrs = crate::imp::net::async_tokio::lookup_host(addr)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "resolve bind address error", e))?;
        let addr = addrs
            .into_iter()
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::Network, "no addresses to bind"))?;
        crate::imp::net::async_tokio::bind_tcp(addr).await.map(Self)
    }

    /// Converts a blocking standard-library listener into an async listener.
    pub fn from_std(listener: std::net::TcpListener) -> RS<Self> {
        crate::imp::net::async_tokio::listener_from_std(listener).map(Self)
    }

    /// Accepts a new connection, returning the stream and peer address.
    pub async fn accept(&self) -> RS<(AsyncTcpStream, SocketAddr)> {
        self.0.accept().await.map(|(s, a)| (AsyncTcpStream(s), a))
    }

    /// Returns the local address this listener is bound to.
    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    /// Unwraps into the underlying [`tokio::net::TcpListener`].
    pub fn into_inner(self) -> tokio::net::TcpListener {
        self.0.into_inner()
    }
}

impl AsyncTcpStream {
    /// Resolves `addr` and connects to the first resolved address.
    pub async fn connect<A: ToAddrs>(addr: A) -> RS<Self> {
        let addrs = crate::imp::net::async_tokio::lookup_host(addr)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "resolve connect address error", e))?;
        let addr = addrs
            .into_iter()
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::Network, "no addresses to connect"))?;
        crate::imp::net::async_tokio::connect_tcp(addr)
            .await
            .map(Self)
    }

    /// Wraps an existing backend stream.
    pub fn new(inner: crate::imp::net::async_tokio::TokioTcpStream) -> Self {
        Self(inner)
    }

    /// Converts a blocking standard-library stream into an async stream.
    pub async fn from_std(stream: std::net::TcpStream) -> RS<Self> {
        crate::imp::net::async_tokio::stream_from_std(stream).map(Self)
    }

    /// Enables or disables `TCP_NODELAY` (Nagle's algorithm).
    pub fn set_nodelay(&self, nodelay: bool) -> RS<()> {
        self.0.set_nodelay(nodelay)
    }

    /// Returns the local address of this connection.
    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    /// Returns the remote peer address of this connection.
    pub fn peer_addr(&self) -> RS<SocketAddr> {
        self.0.peer_addr()
    }

    /// Shuts down the write half of the connection.
    pub async fn shutdown(&mut self) -> RS<()> {
        self.0.shutdown().await
    }

    /// Unwraps into the underlying [`tokio::net::TcpStream`].
    pub fn into_inner(self) -> tokio::net::TcpStream {
        self.0.into_inner()
    }

    /// Converts back into a blocking [`std::net::TcpStream`].
    pub fn into_std(self) -> std::io::Result<std::net::TcpStream> {
        self.0.into_std()
    }
}

impl AsyncRead for AsyncTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for AsyncTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

#[cfg(unix)]
impl AsRawFd for AsyncTcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

#[cfg(unix)]
impl AsRawFd for AsyncTcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
