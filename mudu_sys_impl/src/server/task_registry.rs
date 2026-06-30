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

    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn drain_completions(&self) {
        while let Some(op_id) = self.completion_queue.pop() {
            let Some(task_id) = self.op_registry.lock().unwrap().remove(&op_id) else {
                continue;
            };
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
    }

    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn poll_ready(&self) -> Vec<CompletedWorkerTask> {
        let mut completed = Vec::new();
        while let Some(task_id) = self.ready_queue.pop() {
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
        completed
    }

    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in worker task registry ownership"
    )]
    pub fn is_empty(&self) -> bool {
        self.tasks.lock().unwrap().is_empty()
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
}
