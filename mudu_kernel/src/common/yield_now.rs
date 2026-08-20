//! Cooperative task yield for manually-polled execution contexts.

use std::future::{poll_fn, Future};

/// Yields once, cooperatively, in a way that works on io_uring worker
/// threads.
///
/// `tokio::task::yield_now` must NOT be used there: since tokio 1.52 it
/// defers the current waker into the tokio runtime's defer queue
/// (`runtime::context::defer`) instead of waking it directly, and no tokio
/// scheduler ever runs on the io_uring worker threads (tasks are polled
/// manually by the worker task registry), so a task awaiting it parks
/// forever.
///
/// This future instead wakes the current waker directly and returns
/// `Poll::Pending` exactly once, so any driver that re-polls woken tasks —
/// the worker task registry, a tokio runtime, or `futures::executor` —
/// resumes the awaiting task on a later poll slice.
pub(crate) fn cooperative_yield_now() -> impl Future<Output = ()> {
    let mut yielded = false;
    poll_fn(move |cx| {
        if yielded {
            return std::task::Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn yield_pends_once_then_completes() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            // Driven by a real tokio runtime the helper must complete.
            cooperative_yield_now().await;
            cooperative_yield_now().await;
        })
        .unwrap();
        // Driven by futures::executor (noop waker) it must also complete:
        // the direct wake is registered even if nothing consumes it.
        futures::executor::block_on(async {
            cooperative_yield_now().await;
        });
    }
}
