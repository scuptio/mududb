use crate::sync::a_mutex::AMutex;
use mudu::common::result::RS;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot::{channel, Receiver, Sender};
use tokio::sync::Notify as TokioNotify;

pub fn create_notify_wait<T: Send + Sync + Clone + 'static>() -> (Notify<T>, Wait<T>) {
    let (sender, receiver) = channel();
    (Notify::new(sender), Wait::new(receiver))
}

#[derive(Clone)]
pub struct Notify<T: Send + Sync + Clone + 'static> {
    inner: Arc<Mutex<Option<Sender<T>>>>,
}

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

    pub fn notify(&self, value: T) -> RS<bool> {
        let sender = self.inner.lock().unwrap().take();
        Ok(sender.is_some_and(|sender| sender.send(value).is_ok()))
    }
}

impl<T: Send + Sync + Clone + 'static> Wait<T> {
    fn new(receiver: Receiver<T>) -> Self {
        Self {
            inner: Arc::new(AMutex::new(WaitInner::new(receiver))),
        }
    }

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
