use async_trait::async_trait;
use mudu::common::result::RS;
use std::time::Duration;

/// Async task scheduling contract.
#[async_trait]
pub trait SysTaskAsync: Send + Sync {
    /// Suspend the current task for `dur`.
    async fn sleep(&self, dur: Duration) -> RS<()>;
}
