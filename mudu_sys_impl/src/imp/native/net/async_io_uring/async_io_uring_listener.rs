use crate::contract::async_listener::AsyncListener;
use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_io_uring::async_io_uring_stream::AsyncIoUringStream;
use crate::imp::net::async_io_uring::io_uring_listener::IoUringListener;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::Arc;

pub struct AsyncIoUringListener {
    inner: IoUringListener,
}

impl AsyncIoUringListener {
    pub(crate) fn bind(addr: SocketAddr) -> RS<Self> {
        Ok(Self {
            inner: IoUringListener::bind_listener(addr)?,
        })
    }

    fn new(inner: IoUringListener) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AsyncListener for AsyncIoUringListener {
    fn local_addr(&self) -> RS<SocketAddr> {
        self.inner.local_addr()
    }

    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, SocketAddr)> {
        let (stream, addr) = self.inner.accept().await?;
        Ok((Box::new(AsyncIoUringStream::new(stream)), addr))
    }

    fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.as_raw_fd()
    }

    fn try_clone_listener(&self) -> RS<Arc<dyn AsyncListener>> {
        Ok(Arc::new(Self::new(self.inner.try_clone_listener()?)))
    }
}
