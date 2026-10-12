use super::kernel_invoke_client::KernelInvokeClient;
use super::{AsyncKernelInvokeClient, AsyncKernelInvokeClientFactory};
use async_trait::async_trait;
use mudu::common::result::RS;

/// Factory connecting [`AsyncKernelInvokeClient`] instances to a kernel TCP
/// address.
pub struct KernelInvokeClientFactory;

#[async_trait(?Send)]
impl AsyncKernelInvokeClientFactory for KernelInvokeClientFactory {
    async fn connect(&self, addr: &str) -> RS<Box<dyn AsyncKernelInvokeClient>> {
        Ok(Box::new(KernelInvokeClient::connect(addr).await?))
    }
}
