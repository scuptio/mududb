use super::tokio_tcp_stream::TokioTcpStream;
use crate::imp::net::async_tokio;
use mudu::common::result::RS;
use mudu::error::others::network_error_with_message;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};

pub struct TokioTcpListener {
    inner: tokio::net::TcpListener,
}

impl TokioTcpListener {
    pub fn from_std(listener: std::net::TcpListener) -> RS<Self> {
        tokio::net::TcpListener::from_std(listener)
            .map_err(|e| network_error_with_message(e, "convert std listener to tokio error"))
            .map(async_tokio::TokioTcpListener::new)
    }

    pub(crate) fn new(inner: tokio::net::TcpListener) -> Self {
        Self::from_inner(inner)
    }
    fn from_inner(inner: tokio::net::TcpListener) -> Self {
        Self { inner }
    }

    pub async fn bind(addr: SocketAddr) -> RS<Self> {
        tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| network_error_with_message(e, "bind tokio tcp listener error"))
            .map(Self::from_inner)
    }

    pub async fn accept(&self) -> RS<(TokioTcpStream, SocketAddr)> {
        self.inner
            .accept()
            .await
            .map_err(|e| network_error_with_message(e, "accept tokio tcp stream error"))
            .map(|(stream, addr)| (TokioTcpStream::new(stream), addr))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| network_error_with_message(e, "read tokio listener local addr error"))
    }

    pub fn into_inner(self) -> tokio::net::TcpListener {
        self.inner
    }
}

impl AsRawFd for TokioTcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
