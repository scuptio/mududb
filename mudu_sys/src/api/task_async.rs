use async_trait::async_trait;
use mudu::common::result::RS;
use std::time::Duration;

#[async_trait]
pub trait SysTaskAsync: Send + Sync {
    async fn sleep(&self, dur: Duration) -> RS<()>;
}
