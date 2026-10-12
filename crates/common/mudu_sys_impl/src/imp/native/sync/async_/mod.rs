/// Async task trait objects and their spawn/join wrappers.
pub mod async_task;
/// Async fair read-write lock built from `SMutex` + `ANotify`.
pub mod frwlock;
/// Async mutex backed by `futures::lock::Mutex`.
pub mod futures_mutex;
/// Async mutex backed by `tokio::sync::Mutex`.
pub mod mutex;
/// Async notify with a sticky signaled flag.
pub mod notify;
/// One-shot async notify/wait pair carrying a value.
pub mod notify_wait;
/// Async read-write lock backed by `tokio::sync::RwLock`.
pub mod rwlock;
/// Shared stop-signal channel.
pub mod stop_flag;

pub use async_task::{AsyncLocalTask, AsyncResult, AsyncTask, Task, TaskWrapper};
pub use frwlock::{FRwLock, FRwLockReadGuard, FRwLockWriteGuard};
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

/// Shared notify/wait primitive: the first `notify_all` wakes all current
/// and future waiters (one-shot, sticky).
#[derive(Clone)]
pub struct NotifyWait {
    inner: Arc<NotifyWaitInner>,
}

/// Notifying half of a [`NotifyWait`] pair; cheap to clone, all clones share
/// the same state.
#[derive(Clone)]
pub struct Notifier {
    inner: Arc<NotifyWaitInner>,
}

/// Waiting half of a [`NotifyWait`] pair; cheap to clone, all clones share
/// the same state.
#[derive(Clone)]
pub struct Waiter {
    inner: Arc<NotifyWaitInner>,
}

/// Shared state behind [`NotifyWait`], [`Notifier`] and [`Waiter`].
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

/// Creates a fresh notifier/waiter pair.
pub fn notify_wait() -> (Notifier, Waiter) {
    NotifyWait::new_notify_wait()
}

impl NotifyWait {
    /// Creates a fresh notifier/waiter pair.
    pub fn new_notify_wait() -> (Notifier, Waiter) {
        let inner = Arc::new(NotifyWaitInner::new());
        (
            Notifier {
                inner: inner.clone(),
            },
            Waiter { inner },
        )
    }

    /// Creates another notifier/waiter pair sharing this instance's state.
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

    /// Creates a fresh notify/wait state.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(NotifyWaitInner::new()),
        }
    }

    /// Creates a fresh notify/wait state labeled with `name` for tracing.
    pub fn new_with_name(name: String) -> Self {
        Self {
            inner: Arc::new(NotifyWaitInner::new_with_name(name)),
        }
    }

    /// Returns `true` once `notify_all` has been called on this state.
    pub fn is_notified(&self) -> bool {
        self.inner.is_notified()
    }

    /// Waits until `notify_all` is called; resolves immediately if it
    /// already was.
    pub async fn notified(&self) {
        trace!("notified {}", self.inner.name);
        self.inner.notified().await;
    }

    /// Wakes all current and future waiters; returns `true` if this call
    /// performed the state transition (i.e. it is the first notify).
    pub fn notify_all(&self) -> bool {
        trace!("notify waiter {}", self.inner.name);
        self.inner.notify_all()
    }

    /// Returns a waiter sharing this instance's state.
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
    /// Waits until the shared state is notified; resolves immediately if it
    /// already was.
    pub async fn wait(&self) {
        self.inner.notified().await;
    }

    /// Converts back into the combined [`NotifyWait`] handle.
    pub fn into(self) -> NotifyWait {
        NotifyWait { inner: self.inner }
    }
}

impl Notifier {
    /// Returns `true` once the shared state has been notified.
    pub fn is_notified(&self) -> bool {
        self.inner.is_notified()
    }

    /// Wakes all current and future waiters; returns `true` if this call
    /// performed the state transition.
    pub fn notify_all(&self) -> bool {
        self.inner.notify_all()
    }

    /// Returns a waiter sharing the same state.
    pub fn as_waiter(&self) -> Waiter {
        Waiter {
            inner: self.inner.clone(),
        }
    }

    /// Converts back into the combined [`NotifyWait`] handle.
    pub fn into(self) -> NotifyWait {
        NotifyWait { inner: self.inner }
    }
}
