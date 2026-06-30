use crate::contract::async_listener::AsyncListener;
use crate::contract::async_net::AsyncNet;
use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_io_uring::async_io_uring_listener::AsyncIoUringListener;
use crate::imp::net::async_io_uring::async_io_uring_stream::AsyncIoUringStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Default)]
pub struct AsyncIoUringNet;

impl AsyncIoUringNet {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AsyncNet for AsyncIoUringNet {
    async fn bind_tcp(&self, addr: SocketAddr) -> RS<Arc<dyn AsyncListener>> {
        Ok(Arc::new(AsyncIoUringListener::bind(addr)?))
    }

    async fn connect_tcp(&self, addr: SocketAddr) -> RS<Box<dyn AsyncStream>> {
        Ok(Box::new(AsyncIoUringStream::connect(addr).await?))
    }
}
