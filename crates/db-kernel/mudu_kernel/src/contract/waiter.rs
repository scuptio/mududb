use async_trait::async_trait;
use mudu::common::result::RS;

#[async_trait]
pub trait Waiter<R: Send + Sync + 'static>: Send + Sync {
    async fn wait(&self) -> RS<R>;
}
