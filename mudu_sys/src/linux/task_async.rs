use crate::api::task_async::SysTaskAsync;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::time::Duration;

pub struct LinuxTaskAsync;

#[async_trait]
impl SysTaskAsync for LinuxTaskAsync {
    async fn sleep(&self, dur: Duration) -> RS<()> {
        tokio::time::sleep(dur).await;
        Ok(())
    }
}
