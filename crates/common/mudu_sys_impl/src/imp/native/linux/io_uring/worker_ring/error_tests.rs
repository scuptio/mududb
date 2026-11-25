#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;
use crate::imp::io::user_io::op_state;
use crate::imp::native::linux::io_uring::file::FileCloseRequest;

fn poison<T>(mutex: &Mutex<T>) {
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = mutex.lock().unwrap();
        panic!("injected owner failure");
    }));
    assert!(panic.is_err());
    assert!(mutex.is_poisoned());
}

fn operation() -> WorkerRingOp {
    WorkerRingOp::File(FileIoRequest::Close(FileCloseRequest::new(17, op_state())))
}

#[test]
fn poisoned_operation_map_rejects_enqueue_requeue_and_dequeue() {
    let ring = WorkerLocalRing::new();
    ring.pending.lock().unwrap().push_back(1);
    poison(&ring.ops);
    assert_eq!(
        ring.register(operation()).unwrap_err().ec(),
        ErrorCode::Internal
    );
    assert_eq!(
        ring.requeue_front(1, operation()).unwrap_err().ec(),
        ErrorCode::Internal
    );
    assert_eq!(ring.take_pending().err().unwrap().ec(), ErrorCode::Internal);
}

#[test]
fn poisoned_pending_queue_is_reported_without_panicking() {
    let ring = WorkerLocalRing::new();
    poison(&ring.pending);
    assert_eq!(
        ring.register(operation()).unwrap_err().ec(),
        ErrorCode::Internal
    );
    assert_eq!(
        ring.requeue_front(1, operation()).unwrap_err().ec(),
        ErrorCode::Internal
    );
    assert_eq!(ring.take_pending().err().unwrap().ec(), ErrorCode::Internal);
}

#[test]
fn poisoned_timer_heap_is_reported_by_every_timer_operation() {
    let ring = WorkerLocalRing::new();
    let now = crate::time::instant_now();
    poison(&ring.timeouts);
    assert_eq!(
        ring.register_timeout(now, Waker::noop().clone())
            .unwrap_err()
            .ec(),
        ErrorCode::Internal
    );
    assert_eq!(
        ring.remove_timeout(now, 1).unwrap_err().ec(),
        ErrorCode::Internal
    );
    assert_eq!(
        ring.take_expired_timeouts(now).unwrap_err().ec(),
        ErrorCode::Internal
    );
    assert_eq!(
        ring.next_timeout_deadline().unwrap_err().ec(),
        ErrorCode::Internal
    );
}

#[test]
fn missing_operation_is_detected_and_empty_queue_still_drains() {
    let ring = WorkerLocalRing::default();
    ring.pending.lock().unwrap().push_back(999);
    let error = ring.take_pending().err().unwrap();
    assert_eq!(error.ec(), ErrorCode::Internal);
    assert!(error.to_string().contains("999"));
    assert!(ring.take_pending().unwrap().is_none());
    poison(&ring.op_tasks);
    assert!(ring.task_for_op(999).is_none());
    ring.finish_op(999);
}
