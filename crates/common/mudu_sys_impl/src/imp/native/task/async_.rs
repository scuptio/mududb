use super::id::TaskID;
use mudu::common::result::RS;
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::task_local;

task_local! {
    pub static TASK_ID: TaskID;
}

thread_local! {
    static CURRENT_POLL_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

pub struct PollTaskIdGuard(Option<TaskID>);

impl Drop for PollTaskIdGuard {
    fn drop(&mut self) {
        CURRENT_POLL_TASK_ID.with(|f| f.set(self.0));
    }
}

impl PollTaskIdGuard {
    pub fn enter(task_id: TaskID) -> PollTaskIdGuard {
        PollTaskIdGuard(CURRENT_POLL_TASK_ID.with(|f| f.replace(Some(task_id))))
    }
}

/// 获取当前任务的ID（如果存在）
pub fn try_this_task_id() -> Option<TaskID> {
    TASK_ID.try_with(|f| *f).ok()
}

/// 获取当前任务的ID（必须存在，否则 panic）
#[expect(
    clippy::expect_used,
    reason = "this_task_id is only valid inside a task context; callers should use try_this_task_id for optional access"
)]
pub fn this_task_id() -> TaskID {
    try_this_task_id()
        .expect("cannot access task id: neither tokio task-local nor poll-task TLS is set")
}

/// 获取当前正在poll的任务ID（用于跨线程/LocalSet场景）
pub fn current_poll_task_id() -> Option<TaskID> {
    CURRENT_POLL_TASK_ID.with(|f| f.get())
}

pub struct TaskAsync;

impl TaskAsync {
    pub async fn sleep(dur: Duration) -> RS<()> {
        if crate::io::worker_ring::has_current_worker_ring() {
            // On io_uring worker threads the tokio timer is never driven, so
            // sleep is implemented as a ring-native timeout over a future that
            // never completes.
            let _ = RingTimeout::new(dur, std::future::pending::<()>()).await;
            return Ok(());
        }
        tokio::time::sleep(dur).await;
        Ok(())
    }

    pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
    where
        F: Future,
    {
        if crate::io::worker_ring::has_current_worker_ring() {
            return RingTimeout::new(dur, fut).await;
        }
        tokio::time::timeout(dur, fut).await.ok()
    }
}

/// A `timeout` implementation driven by the worker-local io_uring ring.
///
/// io_uring worker threads run a synchronous service loop inside
/// `block_on`, so the tokio timer wheel never advances there and
/// `tokio::time::timeout` would park the task forever. Instead, the deadline
/// is registered in the worker ring's timeout heap
/// (`WorkerLocalRing::register_timeout`); the service loop pops expired
/// entries each iteration, wakes the task, and bounds its CQE wait by the
/// nearest deadline.
struct RingTimeout<F: Future> {
    fut: Pin<Box<F>>,
    deadline: crate::time::Instant,
    registered: Option<(crate::time::Instant, u64)>,
}

impl<F: Future> RingTimeout<F> {
    fn new(dur: Duration, fut: F) -> Self {
        Self {
            fut: Box::pin(fut),
            deadline: crate::time::instant_now() + dur,
            registered: None,
        }
    }
}

impl<F: Future> Future for RingTimeout<F> {
    type Output = Option<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        // Drop the previous registration first: the waker may have changed
        // since the last poll, and a stale entry would wake nobody useful.
        if let Some((deadline, id)) = this.registered.take() {
            let _ =
                crate::io::worker_ring::with_current_ring(|ring| ring.remove_timeout(deadline, id));
        }
        match this.fut.as_mut().poll(cx) {
            Poll::Ready(output) => Poll::Ready(Some(output)),
            Poll::Pending => {
                let now = crate::time::instant_now();
                if now >= this.deadline {
                    return Poll::Ready(None);
                }
                let deadline = this.deadline;
                if let Ok(id) = crate::io::worker_ring::with_current_ring(|ring| {
                    ring.register_timeout(deadline, cx.waker().clone())
                }) {
                    this.registered = Some((deadline, id));
                }
                Poll::Pending
            }
        }
    }
}

pub async fn sleep(dur: Duration) -> RS<()> {
    TaskAsync::sleep(dur).await
}

pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
where
    F: Future,
{
    TaskAsync::timeout(dur, fut).await
}

pub use super::join_handle::{TaskJoinError, TaskJoinHandle};
pub use super::runtime::{
    block_on_async_current, block_on_tokio_current_thread, build_current_thread_runtime,
    build_multi_thread_runtime, has_tokio_runtime, wait_for_shutdown_signal,
    CurrentThreadTaskRuntime, TaskRuntime, TaskRuntimeEnterGuard,
};
pub use super::spawn::{spawn_blocking, spawn_task, spawn_task_detached};
pub use super::spawn_local::{spawn_local_detached, spawn_local_task, LocalTaskSet, TaskFailed};

#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod ring_timeout_tests {
    use super::*;
    use crate::io::worker_ring::{
        set_current_worker_ring, unset_current_worker_ring, WorkerLocalRing,
    };
    use futures::task::noop_waker;
    use std::sync::Arc;

    struct CurrentRingGuard;

    impl CurrentRingGuard {
        fn new() -> (Self, Arc<WorkerLocalRing>) {
            #[allow(clippy::arc_with_non_send_sync)]
            let ring = Arc::new(WorkerLocalRing::new());
            set_current_worker_ring(ring.clone());
            (Self, ring)
        }
    }

    impl Drop for CurrentRingGuard {
        fn drop(&mut self) {
            unset_current_worker_ring();
        }
    }

    fn poll_once<F: Future>(fut: &mut Pin<Box<F>>) -> Poll<F::Output> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        fut.as_mut().poll(&mut cx)
    }

    #[test]
    fn ring_timeout_returns_some_when_inner_ready() {
        let (_guard, ring) = CurrentRingGuard::new();
        let mut fut = Box::pin(RingTimeout::new(Duration::from_secs(60), async { 42 }));
        match poll_once(&mut fut) {
            Poll::Ready(Some(42)) => {}
            other => panic!("expected Ready(Some(42)), got {:?}", other.is_ready()),
        }
        // An immediately-ready inner future leaves no registration behind.
        assert!(ring.next_timeout_deadline().unwrap().is_none());
    }

    #[test]
    fn ring_timeout_registers_then_fires_after_deadline() {
        let (_guard, ring) = CurrentRingGuard::new();
        let mut fut = Box::pin(RingTimeout::new(
            Duration::from_millis(20),
            std::future::pending::<()>(),
        ));
        // First poll: inner pending, deadline in the future -> parked and registered.
        assert!(matches!(poll_once(&mut fut), Poll::Pending));
        assert!(ring.next_timeout_deadline().unwrap().is_some());
        // Re-poll before the deadline: still pending, exactly one registration.
        assert!(matches!(poll_once(&mut fut), Poll::Pending));
        crate::task::sync::sleep_blocking(Duration::from_millis(40));
        // After the deadline the timeout resolves to None and unregisters.
        assert!(matches!(poll_once(&mut fut), Poll::Ready(None)));
        assert!(ring.next_timeout_deadline().unwrap().is_none());
    }

    #[test]
    fn ring_sleep_resolves_after_deadline() {
        let (_guard, _ring) = CurrentRingGuard::new();
        let mut fut = Box::pin(TaskAsync::sleep(Duration::from_millis(20)));
        assert!(matches!(poll_once(&mut fut), Poll::Pending));
        crate::task::sync::sleep_blocking(Duration::from_millis(40));
        assert!(matches!(poll_once(&mut fut), Poll::Ready(Ok(()))));
    }
}
