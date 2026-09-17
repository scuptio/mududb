use crate::contract::async_listener::AsyncListener;
use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_tokio::async_tokio_stream::AsyncTokioStream;
use crate::imp::net::async_tokio::TokioTcpListener;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
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
        self.inner.local_addr()
    }

    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, SocketAddr)> {
        let (stream, addr) = self.inner.accept().await?;
        Ok((Box::new(AsyncTokioStream::new(stream)), addr))
    }

    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn try_clone_listener(&self) -> RS<Arc<dyn AsyncListener>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "AsyncTokioListener::try_clone_listener is not supported"
        ))
    }
}
