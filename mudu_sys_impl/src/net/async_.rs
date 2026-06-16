use crate::contract::to_addrs::ToAddrs;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct AsyncTcpListener(crate::imp::net::async_tokio::TokioTcpListener);
pub struct AsyncTcpStream(crate::imp::net::async_tokio::TokioTcpStream);

impl AsyncTcpListener {
    pub async fn bind<A: ToAddrs>(addr: A) -> RS<Self> {
        let addrs = crate::imp::net::async_tokio::lookup_host(addr)
            .await
            .map_err(|e| m_error!(EC::NetErr, "resolve bind address error", e))?;
        let addr = addrs
            .into_iter()
            .next()
            .ok_or_else(|| m_error!(EC::NetErr, "no addresses to bind"))?;
        crate::imp::net::async_tokio::bind_tcp(addr).await.map(Self)
    }

    pub fn from_std(listener: std::net::TcpListener) -> RS<Self> {
        crate::imp::net::async_tokio::listener_from_std(listener).map(Self)
    }

    pub async fn accept(&self) -> RS<(AsyncTcpStream, SocketAddr)> {
        self.0.accept().await.map(|(s, a)| (AsyncTcpStream(s), a))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    pub fn into_inner(self) -> tokio::net::TcpListener {
        self.0.into_inner()
    }
}

impl AsyncTcpStream {
    pub async fn connect<A: ToAddrs>(addr: A) -> RS<Self> {
        let addrs = crate::imp::net::async_tokio::lookup_host(addr)
            .await
            .map_err(|e| m_error!(EC::NetErr, "resolve connect address error", e))?;
        let addr = addrs
            .into_iter()
            .next()
            .ok_or_else(|| m_error!(EC::NetErr, "no addresses to connect"))?;
        crate::imp::net::async_tokio::connect_tcp(addr)
            .await
            .map(Self)
    }

    pub fn new(inner: crate::imp::net::async_tokio::TokioTcpStream) -> Self {
        Self(inner)
    }

    pub async fn from_std(stream: std::net::TcpStream) -> RS<Self> {
        crate::imp::net::async_tokio::stream_from_std(stream).map(Self)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> RS<()> {
        self.0.set_nodelay(nodelay)
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    pub fn peer_addr(&self) -> RS<SocketAddr> {
        self.0.peer_addr()
    }

    pub async fn shutdown(&mut self) -> RS<()> {
        self.0.shutdown().await
    }

    pub fn into_inner(self) -> tokio::net::TcpStream {
        self.0.into_inner()
    }

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

impl AsRawFd for AsyncTcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl AsRawFd for AsyncTcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
