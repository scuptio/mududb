use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;
use tracing::trace;

// Notifies tasks to wake up.
// If Notifier::notify_waiters is called, all the task call Notifier::notified would complete, and
// the following invocation of Notifier::notified, which after Notifier::notify_waiters called,
// would return immediately
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
    notify: Notify,
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
}

impl NotifyWaitInner {
    fn new() -> Self {
        Self::new_with_name(Default::default())
    }

    fn new_with_name(name: String) -> Self {
        Self {
            name,
            is_notified: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    async fn notified(&self) {
        if !self.is_notified.load(Ordering::SeqCst) {
            self.notify.notified().await;
        }
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

    pub fn into(self) -> NotifyWait {
        NotifyWait { inner: self.inner }
    }
}
