use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use crossbeam_queue::SegQueue;
use futures::task::waker;
use mudu::common::result::RS;
use mudu_utils::task_async::{try_this_task_id, PollTaskIdGuard};
use mudu_utils::task_context::TaskContext;
use mudu_utils::task_id::{new_task_id, TaskID};
use tracing::trace;

use crate::server::async_func_task_waker::AsyncFuncTaskWaker;
use crate::server::worker_task::{WorkerTask, WorkerTaskFuture};

pub(in crate::server) struct CompletedWorkerTask {
    conn_id: Option<u64>,
    is_system: bool,
    result: RS<()>,
}

pub(crate) struct WorkerTaskRegistry {
    tasks: scc::HashMap<u64, WorkerTask>,
    ready_queue: Arc<SegQueue<u64>>,
    completion_queue: Arc<SegQueue<u64>>,
    op_registry: scc::HashMap<u64, u64>,
    next_task_id: AtomicU64,
    next_op_id: AtomicU64,
}

impl WorkerTaskRegistry {
    pub fn new() -> Self {
        Self {
            tasks: scc::HashMap::new(),
            ready_queue: Arc::new(SegQueue::new()),
            completion_queue: Arc::new(SegQueue::new()),
            op_registry: scc::HashMap::new(),
            next_task_id: AtomicU64::new(1),
            next_op_id: AtomicU64::new(1),
        }
    }

    pub(in crate::server) fn spawn_with_trace_id(
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

    pub(in crate::server) fn spawn(&self, conn_id: Option<u64>, future: WorkerTaskFuture) {
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
    pub(crate) fn spawn_system(&self, future: WorkerTaskFuture) {
        self.spawn(None, future);
    }

    pub(in crate::server) fn drain_completions(&self) {
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

    pub(in crate::server) fn poll_ready(&self) -> Vec<CompletedWorkerTask> {
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

    pub(in crate::server) fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

impl CompletedWorkerTask {
    pub(in crate::server) fn conn_id(&self) -> Option<u64> {
        self.conn_id
    }

    pub(in crate::server) fn is_system(&self) -> bool {
        self.is_system
    }

    pub(in crate::server) fn into_result(self) -> RS<()> {
        self.result
    }
}
