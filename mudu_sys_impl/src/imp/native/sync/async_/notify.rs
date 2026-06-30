use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use tokio::sync::{futures::Notified, Notify as TokioNotify};

pub struct ANotify {
    inner: TokioNotify,
    /// Set to `true` once `notify_waiters()` has been called. This lets
    /// `ANotified` futures created after the notification observe it.
    signaled: AtomicBool,
}

pub struct ANotified<'a> {
    inner: Pin<Box<Notified<'a>>>,
    notify: &'a ANotify,
}

impl ANotify {
    pub fn new() -> Self {
        Self {
            inner: TokioNotify::new(),
            signaled: AtomicBool::new(false),
        }
    }

    pub fn notified(&self) -> ANotified<'_> {
        ANotified {
            inner: Box::pin(self.inner.notified()),
            notify: self,
        }
    }

    pub fn notify_waiters(&self) {
        self.signaled.store(true, Ordering::Release);
        self.inner.notify_waiters();
    }
}

impl Default for ANotify {
    fn default() -> Self {
        Self::new()
    }
}

impl Future for ANotified<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.notify.signaled.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        self.inner.as_mut().poll(cx)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(flavor = "current_thread")]
    async fn anotify_wait_then_notify() {
        let notify = std::sync::Arc::new(ANotify::new());
        let notify2 = notify.clone();
        let handle = tokio::spawn(async move {
            timeout(Duration::from_secs(5), notify2.notified())
                .await
                .expect("notified should resolve in time");
        });
        notify.notify_waiters();
        handle.await.expect("spawned task should complete");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn anotify_pre_notify_wakes_later_waiter() {
        let notify = ANotify::new();
        notify.notify_waiters();
        timeout(Duration::from_secs(5), notify.notified())
            .await
            .expect("pre-notified waiter should resolve immediately");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn anotify_default_uses_new() {
        let notify: ANotify = Default::default();
        drop(notify.notified());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn anotify_multiple_waiters() {
        let notify = std::sync::Arc::new(ANotify::new());
        let notify2 = notify.clone();
        let notify3 = notify.clone();

        let handle1 = tokio::spawn(async move {
            timeout(Duration::from_secs(5), notify2.notified())
                .await
                .expect("first waiter should resolve");
        });
        let handle2 = tokio::spawn(async move {
            timeout(Duration::from_secs(5), notify3.notified())
                .await
                .expect("second waiter should resolve");
        });

        notify.notify_waiters();

        handle1.await.expect("first task should complete");
        handle2.await.expect("second task should complete");
    }
}
