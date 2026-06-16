use mudu::common::result::RS;
use std::future::Future;
use std::time::Duration;

pub async fn sleep(dur: Duration) -> RS<()> {
    crate::imp::task::TaskAsync::sleep(dur).await
}

pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
where
    F: Future,
{
    crate::imp::task::TaskAsync::timeout(dur, fut).await
}

pub enum TaskFailed {
    Cancel,
    Timeout,
}
