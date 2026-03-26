use crossbeam_queue::SegQueue;
use futures::task::ArcWake;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct ProcedureTaskWaker {
    op_id: u64,
    completion_queue: Arc<SegQueue<u64>>,
    completed: Arc<AtomicBool>,
    notified: Arc<AtomicBool>,
}

impl ProcedureTaskWaker {
    pub fn new(
        op_id: u64,
        completion_queue: Arc<SegQueue<u64>>,
        completed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            op_id,
            completion_queue,
            completed,
            notified: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ArcWake for ProcedureTaskWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        // Procedure futures are resumed through the worker's completion path.
        // This mirrors io_uring: wake -> completion queue -> task resume.

        // A wake means the future may now make progress after previously
        // returning `Poll::Pending`. Once the future is already completed,
        // re-queuing it would only create a spurious extra poll.
        if arc_self.completed.load(Ordering::Acquire) {
            return;
        }

        if !arc_self.notified.swap(true, Ordering::AcqRel) {
            arc_self.completion_queue.push(arc_self.op_id);
        }
    }
}
