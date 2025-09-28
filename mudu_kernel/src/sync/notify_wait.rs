use crate::sync::a_mutex::AMutex;
use crate::sync::s_mutex::SMutex;
use mudu::common::result::RS;
use std::sync::Arc;
use tokio::sync::oneshot::{channel, Receiver, Sender};

pub fn create_notify_wait<T: Send + Sync + Clone + 'static>() -> (Notify<T>, Wait<T>) {
    let (s, r) = channel();
    (Notify::<T>::new(s), Wait::<T>::new(r))
}

#[derive(Clone)]
pub struct Notify<T: Send + Sync + Clone + 'static> {
    inner: Arc<SMutex<_LockNotify<T>>>,
}

#[derive(Clone)]
pub struct Wait<T: Send + Sync + Clone + 'static> {
    inner: Arc<AMutex<_LockWait<T>>>,
}

struct _LockNotify<T: Send + Sync + Clone + 'static> {
    notify: Option<Sender<T>>,
}

struct _LockWait<T> {
    wait: Option<Receiver<T>>,
    result: Option<T>,
}

impl<T: Send + Sync + Clone + 'static> Notify<T> {
    fn new(sender: Sender<T>) -> Self {
        Self {
            inner: Arc::new(SMutex::new(_LockNotify::new(sender))),
        }
    }

    pub fn notify(&self, t: T) -> RS<bool> {
        let mut g = self.inner.lock()?;
        g.notify(t)
    }
}

impl<T: Send + Sync + Clone + 'static> Wait<T> {
    fn new(receiver: Receiver<T>) -> Self {
        Self {
            inner: Arc::new(AMutex::new(_LockWait::new(receiver))),
        }
    }

    pub async fn wait(&self) -> RS<Option<T>> {
        let mut guard = self.inner.lock().await;
        guard.wait().await
    }
}

impl<T: Send + Sync + Clone + 'static> _LockWait<T> {
    fn new(ch: Receiver<T>) -> _LockWait<T> {
        Self {
            wait: Some(ch),
            result: None,
        }
    }

    async fn wait(&mut self) -> RS<Option<T>> {
        let mut opt_wait = None;
        std::mem::swap(&mut opt_wait, &mut self.wait);
        match opt_wait {
            Some(recv) => {
                // receive failed is ok
                // for the sender can be dropped
                let r = recv.await;
                match r {
                    Ok(lock_r) => Ok(Some(lock_r)),
                    Err(_) => Ok(None),
                }
            }
            None => match &self.result {
                Some(result) => Ok(Some(result.clone())),
                None => Ok(None),
            },
        }
    }
}

impl<T: Send + Sync + Clone + 'static> _LockNotify<T> {
    fn new(sender: Sender<T>) -> Self {
        Self {
            notify: Some(sender),
        }
    }
    fn notify(&mut self, r: T) -> RS<bool> {
        let mut opt_inner = None;
        std::mem::swap(&mut opt_inner, &mut self.notify);
        match opt_inner {
            Some(inner) => {
                // send failed is ok
                // for the receiver can be dropped
                let r = inner.send(r);
                Ok(r.is_ok())
            }
            None => Ok(false),
        }
    }
}
