use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_tokio::TokioTcpStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::net::SocketAddr;
#[cfg(unix)]
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
            .map_err(|e| mudu_error!(ErrorCode::Network, "read tokio tcp stream error", e))
    }

    async fn write_all(&mut self, buf: &[u8]) -> RS<()> {
        self.inner
            .write_all(buf)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "write tokio tcp stream error", e))
    }

    async fn shutdown(&mut self) -> RS<()> {
        self.inner.shutdown().await
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn set_nodelay(&self) -> RS<()> {
        self.inner.set_nodelay(true)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::AsyncTokioStream;
    use crate::contract::async_stream::AsyncStream;
    use crate::imp::net::async_tokio::{bind_tcp, connect_tcp};
    use crate::net::async_::AsyncTcpStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test(flavor = "current_thread")]
    async fn stream_connect_read_write_shutdown() {
        let listener = bind_tcp("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _peer) = listener.accept().await.unwrap();
            let mut stream = AsyncTcpStream::new(stream);
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
        });

        let mut stream = AsyncTokioStream::connect(addr).await.unwrap();
        stream.set_nodelay().unwrap();
        let _fd = stream.as_raw_fd();
        stream.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf, b"ping");
        stream.shutdown().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stream_new_wraps_a_backend_stream() {
        let listener = bind_tcp("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let backend = connect_tcp(addr).await.unwrap();
        let _stream = AsyncTokioStream::new(backend);
    }
}
