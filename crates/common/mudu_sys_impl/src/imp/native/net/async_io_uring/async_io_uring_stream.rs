use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_io_uring::io_uring_stream::IoUringStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::net::SocketAddr;
use std::os::fd::RawFd;

pub(crate) struct AsyncIoUringStream {
    inner: IoUringStream,
}

impl AsyncIoUringStream {
    pub(crate) async fn connect(addr: SocketAddr) -> RS<Self> {
        Ok(Self {
            inner: IoUringStream::connect(addr).await?,
        })
    }
    pub(crate) fn new(inner: IoUringStream) -> Self {
        Self { inner }
    }
}
#[async_trait]
impl AsyncStream for AsyncIoUringStream {
    async fn read(&mut self, buf: &mut [u8]) -> RS<usize> {
        self.inner.read(buf).await
    }

    async fn write_all(&mut self, buf: &[u8]) -> RS<()> {
        self.inner.write_all(buf).await
    }

    async fn shutdown(&mut self) -> RS<()> {
        self.inner.shutdown().await
    }

    fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.as_raw_fd()
    }

    fn set_nodelay(&self) -> RS<()> {
        self.inner.set_nodelay()
    }
}
