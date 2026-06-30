#![allow(missing_docs)]
pub mod async_task;
pub mod futures_mutex;
pub mod mutex;
pub mod notify;
pub mod notify_wait;
pub mod rwlock;
pub mod stop_flag;

pub use async_task::{AsyncLocalTask, AsyncResult, AsyncTask, Task, TaskWrapper};
pub use futures_mutex::{FMutex, FMutexGuard};
pub use mutex::{AMutex, AMutexGuard, MappedAMutexGuard, TryLockError};
pub use notify::{ANotified, ANotify};
pub use notify_wait::{create_notify_wait, Notify, Wait};
pub use rwlock::{ARwLock, ARwLockReadGuard, ARwLockWriteGuard};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
pub use stop_flag::{stop_channel, StopRx, StopTx};
use tokio::sync::Notify as TokioNotify;
use tracing::trace;

#[derive(Clone)]
pub struct NotifyWait {
    inner: Arc<NotifyWaitInner>,
}

#[derive(Clone)]
pub struct Notifier {
    inner: Arc<NotifyWaitInner>,
}

#[derive(Clone)]
pub struct Waiter {
    inner: Arc<NotifyWaitInner>,
}

pub struct NotifyWaitInner {
    name: String,
    notify: TokioNotify,
    is_notified: AtomicBool,
}

impl Default for NotifyWait {
    fn default() -> Self {
        Self::new()
    }
}

pub fn notify_wait() -> (Notifier, Waiter) {
    NotifyWait::new_notify_wait()
}

impl NotifyWait {
    pub fn new_notify_wait() -> (Notifier, Waiter) {
        let inner = Arc::new(NotifyWaitInner::new());
        (
            Notifier {
                inner: inner.clone(),
            },
            Waiter { inner },
        )
    }

    pub fn notify_wait(&self) -> (Notifier, Waiter) {
        (
            Notifier {
                inner: self.inner.clone(),
            },
            Waiter {
                inner: self.inner.clone(),
            },
        )
    }

    pub fn new() -> Self {
        Self {
            inner: Arc::new(NotifyWaitInner::new()),
        }
    }

    pub fn new_with_name(name: String) -> Self {
        Self {
            inner: Arc::new(NotifyWaitInner::new_with_name(name)),
        }
    }

    pub fn is_notified(&self) -> bool {
        self.inner.is_notified()
    }

    pub async fn notified(&self) {
        trace!("notified {}", self.inner.name);
        self.inner.notified().await;
    }

    pub fn notify_all(&self) -> bool {
        trace!("notify waiter {}", self.inner.name);
        self.inner.notify_all()
    }

    pub fn as_waiter(&self) -> Waiter {
        Waiter {
            inner: self.inner.clone(),
        }
    }
}

impl NotifyWaitInner {
    fn new() -> Self {
        Self::new_with_name(Default::default())
    }

    fn new_with_name(name: String) -> Self {
        Self {
            name,
            is_notified: AtomicBool::new(false),
            notify: TokioNotify::new(),
        }
    }

    async fn notified(&self) {
        if self.is_notified.load(Ordering::SeqCst) {
            return;
        }

        let notified = self.notify.notified();
        tokio::pin!(notified);

        if self.is_notified.load(Ordering::SeqCst) {
            return;
        }

        notified.await;
    }

    fn is_notified(&self) -> bool {
        self.is_notified.load(Ordering::SeqCst)
    }

    fn notify_all(&self) -> bool {
        let r = self
            .is_notified
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);

        match r {
            Ok(_) => {
                self.notify.notify_waiters();
                true
            }
            Err(_) => {
                self.notify.notify_waiters();
                false
            }
        }
    }
}

impl Waiter {
    pub async fn wait(&self) {
        self.inner.notified().await;
    }

    pub fn into(self) -> NotifyWait {
        NotifyWait { inner: self.inner }
    }
}

impl Notifier {
    pub fn is_notified(&self) -> bool {
        self.inner.is_notified()
    }

    pub fn notify_all(&self) -> bool {
        self.inner.notify_all()
    }

    pub fn as_waiter(&self) -> Waiter {
        Waiter {
            inner: self.inner.clone(),
        }
    }

    pub fn into(self) -> NotifyWait {
        NotifyWait { inner: self.inner }
    }
}
