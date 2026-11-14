//! Sendable mailbox for existing relation/transaction interfaces. The tree stays
//! on its creator's LocalSet. Owner-thread calls poll locally once; suspended work
//! remains in a local task, and other threads can only submit owned messages.
use super::PagedRelation;
use futures::{
    future::{AbortHandle, Abortable, LocalBoxFuture},
    task::noop_waker_ref,
};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot};

type Request = Box<dyn FnOnce(Rc<PagedRelation>) + Send>;
thread_local! {
    static LOCAL: RefCell<HashMap<u64, Rc<PagedRelation>>> = RefCell::new(HashMap::new());
}
static NEXT: AtomicU64 = AtomicU64::new(1);

pub(in crate::relation::relation) struct Handle {
    id: u64,
    sender: mpsc::UnboundedSender<Request>,
}
fn stopped() -> mudu::error::MuduError {
    mudu_error!(ErrorCode::ChannelClosed, "paged relation owner stopped")
}

// Only this message reply crosses threads, never the operation future or tree.
pub(in crate::relation::relation) enum Reply<T> {
    Ready(Option<RS<T>>),
    Waiting {
        receiver: oneshot::Receiver<RS<T>>,
        abort: AbortHandle,
    },
}
impl<T: Unpin> Future for Reply<T> {
    type Output = RS<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.get_mut() {
            Self::Ready(value) => Poll::Ready(value.take().unwrap_or_else(|| Err(stopped()))),
            Self::Waiting { receiver, .. } => Pin::new(receiver)
                .poll(cx)
                .map(|r| r.unwrap_or_else(|_| Err(stopped()))),
        }
    }
}
impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        // Abort before the next owner poll so cancelled callers cannot publish
        // a delayed index edit after releasing their relation-level guards.
        if let Self::Waiting { abort, .. } = self {
            abort.abort();
        }
    }
}
struct Registration(u64);
impl Drop for Registration {
    fn drop(&mut self) {
        LOCAL.with(|entries| {
            entries.borrow_mut().remove(&self.0);
        });
    }
}
impl Handle {
    /// Register only after asynchronous open has finished on the owning LocalSet.
    pub(in crate::relation::relation) fn new(relation: PagedRelation) -> RS<Self> {
        let id = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| stopped())?;
        let relation = Rc::new(relation);
        LOCAL.with(|entries| {
            entries.borrow_mut().insert(id, relation.clone());
        });
        let registration = Registration(id);
        let (sender, mut receiver) = mpsc::unbounded_channel::<Request>();
        tokio::task::spawn_local(async move {
            let _registration = registration;
            while let Some(request) = receiver.recv().await {
                request(relation.clone());
            }
        });
        Ok(Self { id, sender })
    }
    pub(in crate::relation::relation) fn call<T: Send + Unpin + 'static>(
        &self,
        operation: impl FnOnce(Rc<PagedRelation>) -> LocalBoxFuture<'static, RS<T>> + Send + 'static,
    ) -> Reply<T> {
        if let Some(relation) = LOCAL.with(|entries| entries.borrow().get(&self.id).cloned()) {
            let mut future = operation(relation);
            // Ready operations never leave the caller task. Pending futures are
            // retained, not cancelled; spawn_local polls them with a real waker.
            if let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(noop_waker_ref()))
            {
                return Reply::Ready(Some(result));
            }
            let (sender, receiver) = oneshot::channel();
            let (abort, registration) = AbortHandle::new_pair();
            tokio::task::spawn_local(async move {
                if let Ok(result) = Abortable::new(future, registration).await {
                    let _ = sender.send(result);
                }
            });
            return Reply::Waiting { receiver, abort };
        }
        let (sender, receiver) = oneshot::channel();
        let (abort, registration) = AbortHandle::new_pair();
        let request: Request = Box::new(move |relation| {
            tokio::task::spawn_local(async move {
                if let Ok(result) =
                    Abortable::new(async move { operation(relation).await }, registration).await
                {
                    let _ = sender.send(result);
                }
            });
        });
        if self.sender.send(request).is_err() {
            return Reply::Ready(Some(Err(stopped())));
        }
        Reply::Waiting { receiver, abort }
    }
}
