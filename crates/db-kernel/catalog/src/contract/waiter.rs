use async_trait::async_trait;
use mudu::common::result::RS;

/// An awaitable handle that eventually yields a result of type `R`.
#[async_trait]
pub trait Waiter<R: Send + Sync + 'static>: Send + Sync {
    /// Waits until the result is available.
    async fn wait(&self) -> RS<R>;
}
