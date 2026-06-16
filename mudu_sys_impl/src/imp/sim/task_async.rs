use mudu::common::result::RS;
use std::future::Future;
use std::time::Duration;

pub struct TaskAsync;

impl TaskAsync {
    pub async fn sleep(_dur: Duration) -> RS<()> {
        // sim: yield but do not wait
        tokio::task::yield_now().await;
        Ok(())
    }

    pub async fn timeout<F>(_dur: Duration, fut: F) -> Option<F::Output>
    where
        F: Future,
    {
        // sim: run the future without timeout
        Some(fut.await)
    }
}
