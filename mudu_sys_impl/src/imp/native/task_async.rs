use mudu::common::result::RS;
use std::future::Future;
use std::time::Duration;

pub struct TaskAsync;

impl TaskAsync {
    pub async fn sleep(dur: Duration) -> RS<()> {
        tokio::time::sleep(dur).await;
        Ok(())
    }

    pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
    where
        F: Future,
    {
        tokio::time::timeout(dur, fut).await.ok()
    }
}
