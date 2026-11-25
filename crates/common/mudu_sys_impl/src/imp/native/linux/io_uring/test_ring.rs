//! Shared test harness: drive a future to completion against a real io_uring
//! ring installed as the current worker ring. Returns `None` (test skips)
//! when the host cannot create a ring.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::task::noop_waker;

use crate::imp::io::worker_ring::{
    complete_user_ring_op, set_current_worker_ring, submit_user_ring_op, unset_current_worker_ring,
    WorkerLocalRing,
};
use crate::imp::native::linux::io_uring::iouring::IoUring;

/// Drives `future` to completion, pumping the ring for every op the future
/// registers with the current worker ring. `None` means io_uring is
/// unavailable on this host and the caller should skip its assertions.
#[allow(clippy::arc_with_non_send_sync)]
pub(crate) fn drive_iouring_future<F, T>(future: F) -> Option<T>
where
    F: Future<Output = T>,
{
    let mut ring = match IoUring::new(16) {
        Ok(ring) => ring,
        Err(err) => {
            eprintln!("skip io_uring test: io_uring unavailable ({err})");
            return None;
        }
    };
    let worker_ring = Arc::new(WorkerLocalRing::new());
    set_current_worker_ring(worker_ring.clone());

    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => {
                unset_current_worker_ring();
                return Some(value);
            }
            Poll::Pending => {
                let Some((op_id, op)) = worker_ring.take_pending().unwrap() else {
                    continue;
                };
                let mut sqe = ring.next_sqe().expect("test io_uring SQE is available");
                sqe.set_user_data(op_id);
                let inflight = submit_user_ring_op(op_id, op, &mut sqe);
                let submit = ring.submit();
                assert!(submit >= 0, "io_uring_submit failed: {submit}");
                let cqe = ring.wait().expect("io_uring_wait_cqe failed");
                assert_eq!(cqe.user_data(), op_id);
                complete_user_ring_op(inflight, cqe.result(), &worker_ring).unwrap();
            }
        }
    }
}
