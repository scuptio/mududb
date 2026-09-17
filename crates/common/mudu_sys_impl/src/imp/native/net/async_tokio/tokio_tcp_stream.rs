use mudu::common::result::RS;
use mudu::error::others::network_error_with_message;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

pub struct TokioTcpStream {
    inner: tokio::net::TcpStream,
}

impl TokioTcpStream {
    pub fn from_std(stream: std::net::TcpStream) -> RS<Self> {
        tokio::net::TcpStream::from_std(stream)
            .map_err(|e| network_error_with_message(e, "convert std stream to tokio error"))
            .map(Self::new)
    }

    pub async fn connect(addr: SocketAddr) -> RS<Self> {
        tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|e| network_error_with_message(e, "connect tokio tcp stream error"))
            .map(|inner| {
                let _ = inner.set_nodelay(true);
                Self::from_inner(inner)
            })
    }

    pub fn new(inner: tokio::net::TcpStream) -> Self {
        Self::from_inner(inner)
    }

    fn from_inner(inner: tokio::net::TcpStream) -> Self {
        Self { inner }
    }

    pub fn set_nodelay(&self, nodelay: bool) -> RS<()> {
        self.inner
            .set_nodelay(nodelay)
            .map_err(|e| network_error_with_message(e, "set tokio tcp nodelay error"))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| network_error_with_message(e, "read tokio tcp local addr error"))
    }

    pub fn peer_addr(&self) -> RS<SocketAddr> {
        self.inner
            .peer_addr()
            .map_err(|e| network_error_with_message(e, "read tokio tcp peer addr error"))
    }

    pub async fn shutdown(&mut self) -> RS<()> {
        AsyncWriteExt::shutdown(&mut self.inner)
            .await
            .map_err(|e| network_error_with_message(e, "shutdown tokio tcp stream error"))
    }

    pub fn into_inner(self) -> tokio::net::TcpStream {
        self.inner
    }

    pub fn into_std(self) -> std::io::Result<std::net::TcpStream> {
        self.inner.into_std()
    }
}

impl AsyncRead for TokioTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for TokioTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl AsRawFd for TokioTcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
