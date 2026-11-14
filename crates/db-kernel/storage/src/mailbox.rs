//! Sendable mailbox confining a `!Send` object to its creator's worker
//! thread. Owner-thread calls poll locally once; suspended work remains in a
//! worker-local task, and other threads can only submit owned messages.
//! Modeled on the relation paged-index handle
//! (`relation/relation/paged/handle.rs`), but the driver and the per-call
//! tasks spawn through `spawn_thread_confined_task`, so both worker backends
//! (tokio `LocalSet` and the io_uring task registry) can drive them.
use futures::{
    future::{AbortHandle, Abortable, LocalBoxFuture},
    task::noop_waker_ref,
};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot};

type Request<T> = Box<dyn FnOnce(Rc<T>) + Send>;

thread_local! {
    // Type-erased per-thread registry: one entry per live confined object.
    static LOCAL: RefCell<HashMap<u64, Rc<dyn Any>>> = RefCell::new(HashMap::new());
}
static NEXT: AtomicU64 = AtomicU64::new(1);

fn stopped() -> mudu::error::MuduError {
    // ChannelClosed doubles as the "confined object's owner is gone"
    // marker: background writers (the periodic dirty-page flush) tolerate
    // it, since a dead owner's data is WAL-covered and nothing remains to
    // write back.
    mudu_error!(ErrorCode::ChannelClosed, "mailbox owner stopped")
}

/// Whether the error reports a mailbox whose owner thread is gone for good.
pub(crate) fn is_owner_stopped(err: &mudu::error::MuduError) -> bool {
    err.ec() == ErrorCode::ChannelClosed
}

struct Registration(u64);
impl Drop for Registration {
    fn drop(&mut self) {
        LOCAL.with(|entries| {
            entries.borrow_mut().remove(&self.0);
        });
    }
}

/// Sendable handle to an object that lives on its creator's worker thread.
/// The owner thread resolves the object from the thread-local registry and
/// polls the operation inline; other threads submit the operation through
/// the channel to a driver task spawned at registration time.
pub(crate) struct Mailbox<T: 'static> {
    id: u64,
    sender: mpsc::UnboundedSender<Request<T>>,
}

impl<T: 'static> Mailbox<T> {
    /// Confines `value` to the current worker thread. Register only after
    /// asynchronous construction has finished on the owning thread. The
    /// calling thread must have a worker ring (io_uring workers) or an active
    /// tokio `LocalSet`; anything else panics the way
    /// `tokio::task::spawn_local` does.
    pub(crate) fn new(value: T) -> RS<Self> {
        let id = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| stopped())?;
        let value = Rc::new(value);
        LOCAL.with(|entries| {
            entries
                .borrow_mut()
                .insert(id, value.clone() as Rc<dyn Any>);
        });
        let registration = Registration(id);
        let (sender, mut receiver) = mpsc::unbounded_channel::<Request<T>>();
        mudu_sys::task::async_::spawn_thread_confined_background_task(
            "mailbox-driver",
            async move {
                let _registration = registration;
                while let Some(request) = receiver.recv().await {
                    request(value.clone());
                }
            },
        )?;
        Ok(Self { id, sender })
    }

    /// Runs `operation` with the confined object. Owner-thread calls are
    /// polled once inline: an immediately-ready operation never leaves the
    /// caller task. See the paged-index handle for the pattern's rationale.
    pub(crate) fn call<R: Send + Unpin + 'static>(
        &self,
        operation: impl FnOnce(Rc<T>) -> LocalBoxFuture<'static, RS<R>> + Send + 'static,
    ) -> Reply<R> {
        let value = LOCAL.with(|entries| {
            entries
                .borrow()
                .get(&self.id)
                .and_then(|erased| erased.clone().downcast::<T>().ok())
        });
        if let Some(value) = value {
            let mut future = operation(value);
            // Ready operations never leave the caller task. Pending futures
            // are retained, not cancelled; a worker-local task polls them
            // with a real waker.
            if let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(noop_waker_ref()))
            {
                return Reply::Ready(Some(result));
            }
            let (sender, receiver) = oneshot::channel();
            let (abort, registration) = AbortHandle::new_pair();
            let spawned =
                mudu_sys::task::async_::spawn_thread_confined_task("mailbox-op", async move {
                    if let Ok(result) = Abortable::new(future, registration).await {
                        let _ = sender.send(result);
                    }
                });
            return match spawned {
                Ok(()) => Reply::Waiting { receiver, abort },
                Err(err) => Reply::Ready(Some(Err(err))),
            };
        }
        let (sender, receiver) = oneshot::channel();
        let (abort, registration) = AbortHandle::new_pair();
        let request: Request<T> = Box::new(move |value| {
            let task = async move {
                if let Ok(result) =
                    Abortable::new(async move { operation(value).await }, registration).await
                {
                    let _ = sender.send(result);
                }
            };
            // On spawn failure the task (and the reply sender inside it) is
            // dropped, so the caller observes a stopped-owner error.
            let _ = mudu_sys::task::async_::spawn_thread_confined_task("mailbox-op", task);
        });
        if self.sender.send(request).is_err() {
            return Reply::Ready(Some(Err(stopped())));
        }
        Reply::Waiting { receiver, abort }
    }
}

/// In-flight mailbox call. Only this message reply crosses threads, never
/// the operation future or the confined object.
pub(crate) enum Reply<T> {
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
            Reply::Ready(value) => Poll::Ready(value.take().unwrap_or_else(|| Err(stopped()))),
            Reply::Waiting { receiver, .. } => Pin::new(receiver)
                .poll(cx)
                .map(|r| r.unwrap_or_else(|_| Err(stopped()))),
        }
    }
}
impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        // Abort before the next owner poll so cancelled callers cannot apply
        // a delayed mutation after releasing their caller-side guards.
        if let Self::Waiting { abort, .. } = self {
            abort.abort();
        }
    }
}
