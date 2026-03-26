use super::tokio_iouring_invoke_client::TokioIoUringInvokeClient;
use super::{AsyncIoUringInvokeClient, AsyncIoUringInvokeClientFactory};
use async_trait::async_trait;
use mudu::common::result::RS;

pub struct TokioIoUringInvokeClientFactory;

#[async_trait(?Send)]
impl AsyncIoUringInvokeClientFactory for TokioIoUringInvokeClientFactory {
    async fn connect(&self, addr: &str) -> RS<Box<dyn AsyncIoUringInvokeClient>> {
        Ok(Box::new(TokioIoUringInvokeClient::connect(addr).await?))
    }
}
