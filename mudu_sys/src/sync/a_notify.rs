use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::sync::{futures::Notified, Notify as TokioNotify};

pub struct ANotify {
    inner: TokioNotify,
}

pub struct ANotified<'a> {
    inner: Pin<Box<Notified<'a>>>,
}

impl ANotify {
    pub fn new() -> Self {
        Self {
            inner: TokioNotify::new(),
        }
    }

    pub fn notified(&self) -> ANotified<'_> {
        ANotified {
            inner: Box::pin(self.inner.notified()),
        }
    }

    pub fn notify_waiters(&self) {
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
        self.inner.as_mut().poll(cx)
    }
}
