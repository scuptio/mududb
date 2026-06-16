use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_tokio::TokioTcpStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct AsyncTokioStream {
    inner: TokioTcpStream,
}

impl AsyncTokioStream {
    pub(crate) async fn connect(addr: SocketAddr) -> RS<AsyncTokioStream> {
        Ok(Self::new(TokioTcpStream::connect(addr).await?))
    }

    pub(crate) fn new(inner: TokioTcpStream) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AsyncStream for AsyncTokioStream {
    async fn read(&mut self, buf: &mut [u8]) -> RS<usize> {
        self.inner
            .read(buf)
            .await
            .map_err(|e| m_error!(EC::NetErr, "read tokio tcp stream error", e))
    }

    async fn write_all(&mut self, buf: &[u8]) -> RS<()> {
        self.inner
            .write_all(buf)
            .await
            .map_err(|e| m_error!(EC::NetErr, "write tokio tcp stream error", e))
    }

    async fn shutdown(&mut self) -> RS<()> {
        self.inner
            .shutdown()
            .await
            .map_err(|e| m_error!(EC::NetErr, "shutdown tokio tcp stream error", e))
    }

    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn set_nodelay(&self) -> RS<()> {
        self.inner
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, "set tokio tcp nodelay error", e))
    }
}
