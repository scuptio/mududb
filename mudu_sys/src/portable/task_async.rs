use crate::api::task_async::SysTaskAsync;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::time::Duration;

pub struct PortableTaskAsync;

#[async_trait]
impl SysTaskAsync for PortableTaskAsync {
    async fn sleep(&self, dur: Duration) -> RS<()> {
        std::thread::sleep(dur);
        Ok(())
    }
}
