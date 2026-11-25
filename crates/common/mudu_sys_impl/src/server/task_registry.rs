use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::sync::SMutex;

use crate::task::async_::{try_this_task_id, PollTaskIdGuard};
use crate::task::context::TaskContext;
use crate::task::id::{new_task_id, TaskID};
use crossbeam_queue::SegQueue;
use futures::task::waker;
use mudu::common::result::RS;
use tracing::trace;

use crate::server::async_func_task_waker::AsyncFuncTaskWaker;
use crate::server::worker_task::{WorkerTask, WorkerTaskFuture};

pub struct CompletedWorkerTask {
    conn_id: Option<u64>,
    is_system: bool,
    result: RS<()>,
}

pub struct WorkerTaskRegistry {
    tasks: SMutex<HashMap<u64, WorkerTask>>,
    ready_queue: Arc<SegQueue<u64>>,
    completion_queue: Arc<SegQueue<u64>>,
    op_registry: SMutex<HashMap<u64, u64>>,
    next_task_id: AtomicU64,
    next_op_id: AtomicU64,
    wake_fd: Option<i32>,
}

impl WorkerTaskRegistry {
    pub fn new_with_wake_fd(wake_fd: Option<i32>) -> Self {
        Self {
            tasks: SMutex::new(HashMap::new()),
            ready_queue: Arc::new(SegQueue::new()),
            completion_queue: Arc::new(SegQueue::new()),
            op_registry: SMutex::new(HashMap::new()),
            next_task_id: AtomicU64::new(1),
            next_op_id: AtomicU64::new(1),
            wake_fd,
        }
    }

    /// Internal: use `mudu_kernel::server::task::spawn` instead.
    #[doc(hidden)]
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn spawn_with_trace_id(
        &self,
        conn_id: Option<u64>,
        trace_task_id: TaskID,
        future: WorkerTaskFuture,
    ) {
        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .tasks
            .lock()
            .unwrap()
            .insert(task_id, WorkerTask::new(conn_id, trace_task_id, future));
        self.ready_queue.push(task_id);
    }

    /// Spawns a perpetual background task that never blocks the shutdown
    /// drain (e.g. a page-buffer mailbox driver idling on its channel). The
    /// task is dropped with the registry when the worker loop exits.
    #[doc(hidden)]
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn spawn_background_with_trace_id(&self, trace_task_id: TaskID, future: WorkerTaskFuture) {
        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .tasks
            .lock()
            .unwrap()
            .insert(task_id, WorkerTask::new_background(trace_task_id, future));
        self.ready_queue.push(task_id);
    }

    /// Internal: use `mudu_kernel::server::task::spawn` instead.
    #[doc(hidden)]
    pub fn spawn(&self, conn_id: Option<u64>, future: WorkerTaskFuture) {
        let task_id = self.next_task_id.load(Ordering::Relaxed);
        let trace_task_id = new_task_id();
        let task_name = match conn_id {
            Some(conn_id) => format!("iouring-task-{task_id}-conn-{conn_id}"),
            None => format!("iouring-system-task-{task_id}"),
        };
        let _ = TaskContext::new_iouring_context(trace_task_id, task_name);
        self.spawn_with_trace_id(conn_id, trace_task_id, future);
    }

    /// Drains completed IO wakes into the ready queue. Returns how many
    /// completions were drained, so event loops that drive the registry
    /// without an armed wake fd (e.g. worker bootstrap) can tell whether
    /// task progress was made.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn drain_completions(&self) -> usize {
        let mut drained = 0usize;
        while let Some(op_id) = self.completion_queue.pop() {
            drained += 1;
            let Some(task_id) = self.op_registry.lock().unwrap().remove(&op_id) else {
                trace!(
                    op_id,
                    "worker_task_registry drain_completions unknown op_id"
                );
                continue;
            };
            trace!(
                op_id,
                task_id,
                "worker_task_registry drain_completions resolved"
            );
            let should_queue = {
                let tasks = self.tasks.lock().unwrap();
                let Some(task) = tasks.get(&task_id) else {
                    continue;
                };
                if let Some(ctx) = TaskContext::get(task.trace_task_id()) {
                    ctx.watch("state", "ready");
                    ctx.watch("wake_op_id", &op_id.to_string());
                }
                !task.queued().swap(true, Ordering::AcqRel)
            };
            if should_queue {
                self.ready_queue.push(task_id);
            }
        }
        drained
    }

    pub fn poll_ready(&self) -> Vec<CompletedWorkerTask> {
        self.poll_ready_budget(usize::MAX).0
    }

    /// Polls at most `budget` ready tasks. The second return value reports
    /// whether more tasks were still queued when the budget ran out, so the
    /// caller (the worker ring loop) can interleave CQE harvesting with task
    /// polling instead of letting one long poll round delay every pending
    /// completion (e.g. WAL fsync) behind it.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn poll_ready_budget(&self, budget: usize) -> (Vec<CompletedWorkerTask>, bool) {
        let mut completed = Vec::new();
        let mut polled = 0usize;
        loop {
            if polled >= budget {
                break;
            }
            let Some(task_id) = self.ready_queue.pop() else {
                break;
            };
            polled += 1;
            let Some(mut task) = self.tasks.lock().unwrap().remove(&task_id) else {
                continue;
            };
            let trace_task_id = task.trace_task_id();
            task.clear_queued();
            if let Some(waiting_on) = task.take_waiting_on() {
                let _ = self.op_registry.lock().unwrap().remove(&waiting_on);
            }
            let op_id = self.next_op_id.fetch_add(1, Ordering::Relaxed);
            // Register the op_id before creating the waker and polling the task.
            // External async operations (e.g. tokio::fs) can complete and call
            // wake() before poll() returns Pending. If the op_id were registered
            // only after the Pending return, that wake would be dropped by
            // drain_completions and the task would stall forever.
            let _ = self.op_registry.lock().unwrap().insert(op_id, task_id);

            let waker = waker(Arc::new(AsyncFuncTaskWaker::new(
                op_id,
                self.completion_queue.clone(),
                task.completed().clone(),
                self.wake_fd,
            )));
            let mut cx = Context::from_waker(&waker);
            let _poll_guard = PollTaskIdGuard::enter(trace_task_id);
            if let Some(ctx) = TaskContext::get(trace_task_id) {
                ctx.watch("state", "polling");
                ctx.watch("poll_task_id", &task_id.to_string());
                if let Some(active_id) = try_this_task_id() {
                    ctx.watch("active_task_id", &active_id.to_string());
                }
            }
            match task.future_mut().poll(&mut cx) {
                Poll::Ready(result) => {
                    // The task completed synchronously; no wake for this op_id
                    // will ever be meaningful, so remove it to avoid leaking a
                    // stale registry entry.
                    let _ = self.op_registry.lock().unwrap().remove(&op_id);
                    task.completed().store(true, Ordering::Release);
                    trace!(
                        task_id,
                        conn_id = task.conn_id(),
                        "worker_task_registry task ready"
                    );
                    completed.push(CompletedWorkerTask {
                        conn_id: task.conn_id(),
                        is_system: task.conn_id().is_none(),
                        result,
                    })
                }
                Poll::Pending => {
                    trace!(
                        task_id,
                        conn_id = task.conn_id(),
                        op_id,
                        "worker_task_registry task pending"
                    );
                    task.set_waiting_on(op_id);
                    if let Some(ctx) = TaskContext::get(trace_task_id) {
                        ctx.watch("state", "pending");
                        ctx.watch("waiting_waker_op_id", &op_id.to_string());
                    }
                    let _ = self.tasks.lock().unwrap().insert(task_id, task);
                    continue;
                }
            }
            TaskContext::remove_context(trace_task_id);
        }
        let more_ready = !self.ready_queue.is_empty();
        (completed, more_ready)
    }

    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn is_empty(&self) -> bool {
        self.tasks.lock().unwrap().is_empty()
    }

    /// Whether only background tasks remain: perpetual listeners are dropped
    /// with the registry at loop exit, so the shutdown drain must not wait
    /// for them.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn is_drained_of_blocking(&self) -> bool {
        self.tasks
            .lock()
            .unwrap()
            .values()
            .all(|task| !task.is_blocking())
    }
}

impl CompletedWorkerTask {
    pub fn conn_id(&self) -> Option<u64> {
        self.conn_id
    }

    pub fn is_system(&self) -> bool {
        self.is_system
    }

    pub fn into_result(self) -> RS<()> {
        self.result
    }
}

#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use futures::channel::oneshot;

    #[test]
    fn external_wake_notifies_worker_eventfd_and_repolls_task() {
        let fd = crate::sync::sync_::blocking::eventfd().unwrap();
        let registry = WorkerTaskRegistry::new_with_wake_fd(Some(fd));
        let (tx, rx) = oneshot::channel::<()>();

        registry.spawn(
            None,
            Box::pin(async move {
                rx.await.unwrap();
                Ok(())
            }),
        );

        assert!(registry.poll_ready().is_empty());

        tx.send(()).unwrap();
        assert_eq!(crate::sync::sync_::blocking::read_eventfd(fd).unwrap(), 1);

        registry.drain_completions();
        let completed = registry.poll_ready();
        assert_eq!(completed.len(), 1);
        completed.into_iter().next().unwrap().into_result().unwrap();

        crate::sync::sync_::blocking::close_fd(fd).unwrap();
    }
    #[test]
    fn budgeted_polling_preserves_connection_results_and_background_drain() {
        let registry = WorkerTaskRegistry::new_with_wake_fd(None);
        assert!(registry.is_empty());
        assert!(registry.is_drained_of_blocking());
        registry.spawn(
            Some(17),
            Box::pin(async {
                Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Io,
                    "request failed"
                ))
            }),
        );
        registry.spawn(None, Box::pin(async { Ok(()) }));
        let background_id = new_task_id();
        registry.spawn_background_with_trace_id(background_id, Box::pin(std::future::pending()));
        assert!(!registry.is_empty());
        assert!(!registry.is_drained_of_blocking());
        let (completed, more) = registry.poll_ready_budget(0);
        assert!(completed.is_empty());
        assert!(more);
        let (mut completed, more) = registry.poll_ready_budget(1);
        assert!(more);
        let completed = completed.pop().unwrap();
        assert_eq!(completed.conn_id(), Some(17));
        assert!(!completed.is_system());
        assert_eq!(
            completed.into_result().unwrap_err().ec(),
            mudu::error::ErrorCode::Io
        );
        let (mut completed, more) = registry.poll_ready_budget(2);
        assert!(!more);
        assert_eq!(completed.len(), 1);
        let completed = completed.pop().unwrap();
        assert_eq!(completed.conn_id(), None);
        assert!(completed.is_system());
        completed.into_result().unwrap();
        assert!(!registry.is_empty());
        assert!(registry.is_drained_of_blocking());
        assert!(registry.poll_ready().is_empty());
    }

    #[test]
    fn stale_completions_and_duplicate_wakes_do_not_poll_twice() {
        let registry = WorkerTaskRegistry::new_with_wake_fd(None);
        let polls = Arc::new(AtomicU64::new(0));
        let seen = polls.clone();
        registry.spawn(
            Some(1),
            Box::pin(std::future::poll_fn(move |cx| {
                if seen.fetch_add(1, Ordering::Relaxed) == 0 {
                    cx.waker().wake_by_ref();
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(Ok(()))
                }
            })),
        );
        assert!(registry.poll_ready().is_empty());
        registry.completion_queue.push(u64::MAX);
        registry.drain_completions();
        assert_eq!(registry.poll_ready().len(), 1);
        assert_eq!(polls.load(Ordering::Relaxed), 2);
        assert!(registry.is_empty());
        assert_eq!(registry.drain_completions(), 0);
        assert!(registry.poll_ready().is_empty());
    }
}
