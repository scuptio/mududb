use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;
use tracing::trace;

// Notifies tasks to wake up.
// If Notifier::notify_waiters is called, all the task call Notifier::notified would complete, and
// the following invocation of Notifier::notified, which after Notifier::notify_waiters called,
// would return immediately
#[derive(Clone)]
pub struct Notifier {
    name: String,
    inner: Arc<NotifyInner>,
}

pub struct NotifyInner {
    stop_notifier: Notify,
    stopped: AtomicBool,
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier {
    pub fn new() -> Self {
        Self {
            name: "".to_string(),
            inner: Arc::new(NotifyInner::new()),
        }
    }

    pub fn new_with_name(name: String) -> Self {
        Self {
            name,
            inner: Arc::new(NotifyInner::new()),
        }
    }

    pub fn is_notified(&self) -> bool {
        self.inner.is_notified()
    }

    pub async fn notified(&self) {
        trace!("notified {}", self.name);
        self.inner.notified().await;
        trace!("notified {} done", self.name);
    }

    pub fn task_notify_all(&self) -> bool {
        trace!("notify waiter {}", self.name);

        self.inner.notify_all()
    }

    pub fn notify_all(&self) -> bool {
        trace!("notify waiter {}", self.name);

        self.inner.notify_all()
    }
}

impl NotifyInner {
    fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
            stop_notifier: Notify::new(),
        }
    }

    async fn notified(&self) {
        if !self.stopped.load(Ordering::SeqCst) {
            self.stop_notifier.notified().await;
        }
    }

    fn is_notified(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    fn notify_all(&self) -> bool {
        let r = self
            .stopped
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);

        match r {
            Ok(_) => {
                self.stop_notifier.notify_waiters();
                true
            }
            Err(_) => {
                self.stop_notifier.notify_waiters();
                false
            }
        }
    }
}
