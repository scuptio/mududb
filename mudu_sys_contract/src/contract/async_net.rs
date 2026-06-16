use crate::contract::async_listener::AsyncListener;
use crate::contract::async_stream::AsyncStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

#[async_trait]
pub trait AsyncNet: Send + Sync {
    async fn bind_tcp(&self, _addr: std::net::SocketAddr) -> RS<Arc<dyn AsyncListener>> {
        Err(mudu::m_error!(
            mudu::error::ec::EC::NotImplemented,
            "async net bind_tcp is not implemented"
        ))
    }

    async fn connect_tcp(&self, _addr: std::net::SocketAddr) -> RS<Box<dyn AsyncStream>> {
        Err(mudu::m_error!(
            mudu::error::ec::EC::NotImplemented,
            "async net connect_tcp is not implemented"
        ))
    }
}
