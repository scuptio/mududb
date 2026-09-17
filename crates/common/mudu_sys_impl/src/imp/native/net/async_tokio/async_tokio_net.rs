use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::contract::async_listener::AsyncListener;
use crate::contract::async_net::AsyncNet;
use crate::contract::async_stream::AsyncStream;
use crate::imp::net::async_tokio::async_tokio_listener::AsyncTokioListener;
use crate::imp::net::async_tokio::async_tokio_stream::AsyncTokioStream;
use mudu::common::result::RS;

#[derive(Default)]
pub struct TokioNet;

impl TokioNet {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AsyncNet for TokioNet {
    async fn bind_tcp(&self, addr: SocketAddr) -> RS<Arc<dyn AsyncListener>> {
        Ok(Arc::new(AsyncTokioListener::bind(addr).await?))
    }

    async fn connect_tcp(&self, addr: SocketAddr) -> RS<Box<dyn AsyncStream>> {
        Ok(Box::new(AsyncTokioStream::connect(addr).await?))
    }
}
