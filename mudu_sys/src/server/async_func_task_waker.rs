use crossbeam_queue::SegQueue;
use futures::task::ArcWake;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::trace;

pub struct AsyncFuncTaskWaker {
    op_id: u64,
    completion_queue: Arc<SegQueue<u64>>,
    completed: Arc<AtomicBool>,
    notified: Arc<AtomicBool>,
    wake_fd: Option<i32>,
}

impl AsyncFuncTaskWaker {
    pub fn new(
        op_id: u64,
        completion_queue: Arc<SegQueue<u64>>,
        completed: Arc<AtomicBool>,
        wake_fd: Option<i32>,
    ) -> Self {
        Self {
            op_id,
            completion_queue,
            completed,
            notified: Arc::new(AtomicBool::new(false)),
            wake_fd,
        }
    }
}

impl ArcWake for AsyncFuncTaskWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        if arc_self.completed.load(Ordering::Acquire) {
            return;
        }

        if !arc_self.notified.swap(true, Ordering::AcqRel) {
            arc_self.completion_queue.push(arc_self.op_id);
            // External async completions can wake this task while the worker
            // thread is blocked in io_uring_wait_cqe. Queueing the task alone
            // is not enough; nudge the worker mailbox eventfd so the ring loop
            // wakes, drains completions, and polls the task again.
            if let Some(fd) = arc_self.wake_fd {
                if let Err(err) = crate::sync_sync::notify_eventfd(fd) {
                    trace!(fd, error = %err, "worker task waker eventfd notify failed");
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use futures::task::ArcWake;

    #[test]
    fn worker_task_waker_notifies_eventfd() {
        let fd = crate::sync_sync::eventfd().unwrap();
        let completion_queue = Arc::new(SegQueue::new());
        let completed = Arc::new(AtomicBool::new(false));
        let waker = Arc::new(AsyncFuncTaskWaker::new(
            42,
            completion_queue.clone(),
            completed,
            Some(fd),
        ));

        AsyncFuncTaskWaker::wake_by_ref(&waker);

        assert_eq!(completion_queue.pop(), Some(42));
        assert_eq!(crate::sync_sync::read_eventfd(fd).unwrap(), 1);
        crate::sync_sync::close_fd(fd).unwrap();
    }
}
