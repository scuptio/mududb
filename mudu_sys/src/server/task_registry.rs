use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use crossbeam_queue::SegQueue;
use futures::task::waker;
use mudu::common::result::RS;
use crate::task_async::{PollTaskIdGuard, try_this_task_id};
use crate::task_context::TaskContext;
use crate::task_id::{TaskID, new_task_id};
use tracing::trace;

use crate::server::async_func_task_waker::AsyncFuncTaskWaker;
use crate::server::worker_task::{WorkerTask, WorkerTaskFuture};

pub struct CompletedWorkerTask {
    conn_id: Option<u64>,
    is_system: bool,
    result: RS<()>,
}

pub struct WorkerTaskRegistry {
    tasks: scc::HashMap<u64, WorkerTask>,
    ready_queue: Arc<SegQueue<u64>>,
    completion_queue: Arc<SegQueue<u64>>,
    op_registry: scc::HashMap<u64, u64>,
    next_task_id: AtomicU64,
    next_op_id: AtomicU64,
    wake_fd: Option<i32>,
}

impl WorkerTaskRegistry {
    pub fn new_with_wake_fd(wake_fd: Option<i32>) -> Self {
        Self {
            tasks: scc::HashMap::new(),
            ready_queue: Arc::new(SegQueue::new()),
            completion_queue: Arc::new(SegQueue::new()),
            op_registry: scc::HashMap::new(),
            next_task_id: AtomicU64::new(1),
            next_op_id: AtomicU64::new(1),
            wake_fd,
        }
    }

    pub fn spawn_with_trace_id(
        &self,
        conn_id: Option<u64>,
        trace_task_id: TaskID,
        future: WorkerTaskFuture,
    ) {
        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .tasks
            .insert_sync(task_id, WorkerTask::new(conn_id, trace_task_id, future));
        self.ready_queue.push(task_id);
    }

    pub fn spawn(&self, conn_id: Option<u64>, future: WorkerTaskFuture) {
        let task_id = self.next_task_id.load(Ordering::Relaxed);
        let trace_task_id = new_task_id();
        let task_name = match conn_id {
            Some(conn_id) => format!("iouring-task-{task_id}-conn-{conn_id}"),
            None => format!("iouring-system-task-{task_id}"),
        };
        let _ = TaskContext::new_context(trace_task_id, task_name, false);
        self.spawn_with_trace_id(conn_id, trace_task_id, future);
    }

    #[allow(dead_code)]
    pub fn spawn_system(&self, future: WorkerTaskFuture) {
        self.spawn(None, future);
    }

    pub fn drain_completions(&self) {
        while let Some(op_id) = self.completion_queue.pop() {
            let Some((_, task_id)) = self.op_registry.remove_sync(&op_id) else {
                continue;
            };
            let Some(task) = self.tasks.get_sync(&task_id) else {
                continue;
            };
            if let Some(ctx) = TaskContext::get(task.trace_task_id()) {
                ctx.watch("state", "ready");
                ctx.watch("wake_op_id", &op_id.to_string());
            }
            if !task.queued().swap(true, Ordering::AcqRel) {
                self.ready_queue.push(task_id);
            }
        }
    }

    pub fn poll_ready(&self) -> Vec<CompletedWorkerTask> {
        let mut completed = Vec::new();
        while let Some(task_id) = self.ready_queue.pop() {
            let Some((_, mut task)) = self.tasks.remove_sync(&task_id) else {
                continue;
            };
            let trace_task_id = task.trace_task_id();
            task.clear_queued();
            if let Some(waiting_on) = task.take_waiting_on() {
                let _ = self.op_registry.remove_sync(&waiting_on);
            }
            let op_id = self.next_op_id.fetch_add(1, Ordering::Relaxed);

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
                    let _ = self.op_registry.insert_sync(op_id, task_id);
                    let _ = self.tasks.insert_sync(task_id, task);
                    continue;
                }
            }
            TaskContext::remove_context(trace_task_id);
        }
        completed
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
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
mod tests {
    use super::*;
    use futures::channel::oneshot;

    #[test]
    fn external_wake_notifies_worker_eventfd_and_repolls_task() {
        let fd = crate::sync_sync::eventfd().unwrap();
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
        assert_eq!(crate::sync_sync::read_eventfd(fd).unwrap(), 1);

        registry.drain_completions();
        let completed = registry.poll_ready();
        assert_eq!(completed.len(), 1);
        completed.into_iter().next().unwrap().into_result().unwrap();

        crate::sync_sync::close_fd(fd).unwrap();
    }
}
