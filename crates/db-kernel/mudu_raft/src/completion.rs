//! Single-consumer completions usable from synchronous and asynchronous callers.

use std::sync::Arc;
use std::time::Duration;

use mudu::common::result::RS;
use mudu_sys::sync::sync_::channel::{sync_channel, SyncSender};
use mudu_sys::sync::sync_::unbounded_channel::SyncReceiver;
use mudu_sys::tokio::sync::Notify;

/// Nonblocking sending half of a one-shot completion.
pub struct CompletionSender<T> {
    tx: Option<SyncSender<T>>,
    ready: Arc<Notify>,
    budget: Option<mudu_sys::tokio::sync::OwnedSemaphorePermit>,
}

/// Receiving half of a one-shot completion.
pub struct CompletionReceiver<T> {
    rx: SyncReceiver<T>,
    ready: Arc<Notify>,
}

/// Creates a matched one-shot completion channel.
pub fn completion<T>() -> (CompletionSender<T>, CompletionReceiver<T>) {
    let (tx, rx) = sync_channel(1);
    let ready = Arc::new(Notify::new());
    (
        CompletionSender {
            tx: Some(tx),
            ready: ready.clone(),
            budget: None,
        },
        CompletionReceiver { rx, ready },
    )
}

impl<T> CompletionSender<T> {
    /// Attaches a pre-acquired semaphore permit to budget this completion.
    pub fn reserve(&mut self, budget: mudu_sys::tokio::sync::OwnedSemaphorePermit) {
        self.budget = Some(budget);
    }
    /// Publishes the result without blocking the protocol owner.
    pub fn send(mut self, value: T) -> RS<()> {
        let result = match self.tx.take() {
            Some(tx) => tx.try_send(value),
            None => {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::ChannelClosed,
                    "completion sender closed"
                ))
            }
        };
        self.ready.notify_one();
        result
    }
}

impl<T> Drop for CompletionSender<T> {
    fn drop(&mut self) {
        // Close the channel before waking: the receiver must see either a
        // result or disconnection even if the producer failed before send.
        self.tx.take();
        self.budget.take();
        self.ready.notify_one();
    }
}

impl<T> CompletionReceiver<T> {
    /// Blocks until the result is delivered.
    pub fn recv(&self) -> RS<T> {
        self.rx.recv()
    }

    /// Blocks up to `duration`; `Ok(None)` on timeout.
    pub fn recv_timeout(&self, duration: Duration) -> RS<Option<T>> {
        self.rx.recv_timeout(duration)
    }

    /// Awaits the result without blocking an executor thread.
    pub async fn wait(&mut self) -> RS<T> {
        loop {
            // notify_one retains a permit when no waiter is registered.
            // There is only one consumer, so cancellation cannot lose a result.
            let notified = self.ready.notified();
            if let Some(value) = self.rx.try_recv()? {
                return Ok(value);
            }
            notified.await;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    #[test]
    fn completion_survives_cancelled_wait_and_early_notification() {
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = completion();
            {
                let mut waiting = Box::pin(rx.wait());
                let mut context = Context::from_waker(Waker::noop());
                assert!(matches!(waiting.as_mut().poll(&mut context), Poll::Pending));
            }
            tx.send(7).unwrap();
            assert_eq!(rx.wait().await.unwrap(), 7);
        });
    }

    #[test]
    fn producer_drop_wakes_async_waiter_with_error() {
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = completion::<()>();
            let waiter = mudu_sys::tokio::spawn(async move { rx.wait().await });
            mudu_sys::tokio::task::yield_now().await;
            drop(tx);
            assert!(waiter.await.unwrap().is_err());
        });
    }
}
