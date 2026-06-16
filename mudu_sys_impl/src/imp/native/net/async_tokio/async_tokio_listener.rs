use crate::contract::async_listener::AsyncListener;
use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_tokio::TokioTcpListener;
use crate::imp::net::async_tokio::async_tokio_stream::AsyncTokioStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
use std::sync::Arc;

pub struct AsyncTokioListener {
    inner: TokioTcpListener,
}

impl AsyncTokioListener {
    pub(crate) async fn bind(addr: SocketAddr) -> RS<Self> {
        Ok(Self {
            inner: TokioTcpListener::bind(addr).await?,
        })
    }
}

#[async_trait]
impl AsyncListener for AsyncTokioListener {
    fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| m_error!(EC::NetErr, "read tokio listener local addr error", e))
    }

    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, SocketAddr)> {
        let (stream, addr) = self.inner.accept().await?;
        Ok((Box::new(AsyncTokioStream::new(stream)), addr))
    }

    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn try_clone_listener(&self) -> RS<Arc<dyn AsyncListener>> {
        todo!()
    }
}
