use crate::contract::async_listener::AsyncListener;
use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_tokio::async_tokio_stream::AsyncTokioStream;
use crate::imp::net::async_tokio::TokioTcpListener;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::net::SocketAddr;
#[cfg(unix)]
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

    #[cfg(unix)]
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::AsyncTokioListener;
    use crate::contract::async_listener::AsyncListener;

    #[tokio::test(flavor = "current_thread")]
    async fn listener_bind_accept_raw_fd_and_clone_unsupported() {
        let listener = AsyncTokioListener::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let _fd = listener.as_raw_fd();
        assert!(listener.try_clone_listener().is_err());

        let server = tokio::spawn(async move {
            let (stream, _peer) = listener.accept().await.unwrap();
            stream
        });
        let _client = crate::imp::net::async_tokio::connect_tcp(addr)
            .await
            .unwrap();
        let _stream = server.await.unwrap();
    }
}
