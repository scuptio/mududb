use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct TaskJoinError(tokio::task::JoinError);

impl TaskJoinError {
    pub fn into_external(self) -> tokio::task::JoinError {
        self.0
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    pub fn is_panic(&self) -> bool {
        self.0.is_panic()
    }
}

impl std::fmt::Debug for TaskJoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for TaskJoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for TaskJoinError {}

impl From<tokio::task::JoinError> for TaskJoinError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self(value)
    }
}

pub struct TaskJoinHandle<T>(tokio::task::JoinHandle<T>);

impl<T> TaskJoinHandle<T> {
    pub fn new(inner: tokio::task::JoinHandle<T>) -> Self {
        Self(inner)
    }

    pub fn abort(&self) {
        self.0.abort();
    }

    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    pub fn into_external(self) -> tokio::task::JoinHandle<T> {
        self.0
    }
}

impl<T> Future for TaskJoinHandle<T> {
    type Output = Result<T, TaskJoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx).map_err(TaskJoinError::from)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn await_ok_result() {
        let handle = TaskJoinHandle::new(tokio::spawn(async { 42 }));
        assert_eq!(handle.await.unwrap(), 42);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn abort_reports_cancelled() {
        let handle = TaskJoinHandle::new(tokio::spawn(async {
            loop {
                tokio::task::yield_now().await;
            }
        }));
        handle.abort();
        let err = handle.await.unwrap_err();
        assert!(err.is_cancelled());
        assert!(!err.is_panic());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn is_finished_before_after_completion_and_after_abort() {
        let mut handle = TaskJoinHandle::new(tokio::spawn(async { 7 }));
        assert!(!handle.is_finished());
        assert_eq!((&mut handle).await.unwrap(), 7);
        assert!(handle.is_finished());

        let mut handle = TaskJoinHandle::new(tokio::spawn(async {
            loop {
                tokio::task::yield_now().await;
            }
        }));
        handle.abort();
        let _ = (&mut handle).await;
        assert!(handle.is_finished());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn into_external_can_await() {
        let handle = TaskJoinHandle::new(tokio::spawn(async { 99 }));
        let ext = handle.into_external();
        assert_eq!(ext.await.unwrap(), 99);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn task_join_error_preserves_cancellation_and_panic() {
        let cancelled = TaskJoinHandle::new(tokio::spawn(async {
            loop {
                tokio::task::yield_now().await;
            }
        }));
        cancelled.abort();
        let err = cancelled.await.unwrap_err();
        assert!(err.is_cancelled());
        assert!(!err.is_panic());
        assert!(!format!("{err}").is_empty());
        assert!(!format!("{err:?}").is_empty());

        let panicked = TaskJoinHandle::new(tokio::spawn(async {
            panic!("boom");
        }));
        let err = panicked.await.unwrap_err();
        assert!(err.is_panic());
        assert!(!err.is_cancelled());
        assert!(!format!("{err}").is_empty());
        assert!(!format!("{err:?}").is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn panic_task_reports_panic() {
        let handle = TaskJoinHandle::new(tokio::spawn(async {
            panic!("task panic");
        }));
        let err = handle.await.unwrap_err();
        assert!(err.is_panic());
        assert!(!err.is_cancelled());
    }
}
