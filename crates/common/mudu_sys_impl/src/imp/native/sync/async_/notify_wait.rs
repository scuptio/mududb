use super::mutex::AMutex;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot::{channel, Receiver, Sender};
use tokio::sync::Notify as TokioNotify;

/// Creates a one-shot notify/wait pair that can carry a value of type `T`.
pub fn create_notify_wait<T: Send + Sync + Clone + 'static>() -> (Notify<T>, Wait<T>) {
    let (sender, receiver) = channel();
    (Notify::new(sender), Wait::new(receiver))
}

/// Notifying half of a [`create_notify_wait`] pair.
#[derive(Clone)]
pub struct Notify<T: Send + Sync + Clone + 'static> {
    inner: Arc<Mutex<Option<Sender<T>>>>,
}

/// Waiting half of a [`create_notify_wait`] pair; multiple clones observe
/// the same single result.
#[derive(Clone)]
pub struct Wait<T: Send + Sync + Clone + 'static> {
    inner: Arc<AMutex<WaitInner<T>>>,
}

struct WaitInner<T> {
    wait: Option<Receiver<T>>,
    result: Option<Option<T>>,
    ready: Arc<TokioNotify>,
}

impl<T: Send + Sync + Clone + 'static> Notify<T> {
    fn new(sender: Sender<T>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(sender))),
        }
    }

    /// Delivers `value` to the waiters; returns `true` if a receiver was
    /// still present. Only the first call across all clones can succeed.
    pub fn notify(&self, value: T) -> RS<bool> {
        let sender = self
            .inner
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Mutex, "notify wait mutex poisoned"))?
            .take();
        Ok(sender.is_some_and(|sender| sender.send(value).is_ok()))
    }
}

impl<T: Send + Sync + Clone + 'static> Wait<T> {
    fn new(receiver: Receiver<T>) -> Self {
        Self {
            inner: Arc::new(AMutex::new(WaitInner::new(receiver))),
        }
    }

    /// Waits for the notified value; `Ok(None)` means the notifier was
    /// dropped without notifying. All clones of this waiter observe the
    /// same result.
    pub async fn wait(&self) -> RS<Option<T>> {
        loop {
            enum Action<T> {
                Return(Option<T>),
                AwaitReceiver(Receiver<T>),
                AwaitReady(Arc<TokioNotify>),
            }

            let action = {
                let mut guard = self.inner.lock().await;
                if let Some(result) = guard.result.clone() {
                    Action::Return(result)
                } else if let Some(wait) = guard.wait.take() {
                    Action::AwaitReceiver(wait)
                } else {
                    Action::AwaitReady(guard.ready.clone())
                }
            };

            match action {
                Action::Return(result) => return Ok(result),
                Action::AwaitReceiver(wait) => {
                    let result = wait.await.ok();
                    let mut guard = self.inner.lock().await;
                    guard.result = Some(result.clone());
                    guard.ready.notify_waiters();
                    return Ok(result);
                }
                Action::AwaitReady(ready) => ready.notified().await,
            }
        }
    }
}

impl<T> WaitInner<T> {
    fn new(wait: Receiver<T>) -> Self {
        Self {
            wait: Some(wait),
            result: None,
            ready: Arc::new(TokioNotify::new()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::create_notify_wait;

    #[tokio::test(flavor = "current_thread")]
    async fn notify_delivers_one_value_to_every_waiter() {
        let (notify, wait) = create_notify_wait::<u32>();
        let second_waiter = wait.clone();
        let second = tokio::spawn(async move { second_waiter.wait().await });

        // Yield so the second waiter registers before the notification.
        tokio::task::yield_now().await;
        assert!(notify.notify(7).unwrap());
        // Only the first notify across all clones can succeed.
        assert!(!notify.notify(8).unwrap());

        assert_eq!(second.await.unwrap().unwrap(), Some(7));
        // The result is cached: later waits return the same value.
        assert_eq!(wait.wait().await.unwrap(), Some(7));
        assert_eq!(wait.wait().await.unwrap(), Some(7));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wait_returns_none_when_notifier_is_dropped() {
        let (notify, wait) = create_notify_wait::<u32>();
        drop(notify);
        assert_eq!(wait.wait().await.unwrap(), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn notify_reports_false_after_the_receiver_is_dropped() {
        let (notify, wait) = create_notify_wait::<u32>();
        drop(wait);
        assert!(!notify.notify(1).unwrap());
    }
}
