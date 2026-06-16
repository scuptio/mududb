#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use futures::task::noop_waker;

    use crate::imp::fs::async_io_uring::iou_create_dir_all;
    use crate::io::iouring::IoUring;
    use crate::io::worker_ring::{
        WorkerLocalRing, complete_user_ring_op, set_current_worker_ring, submit_user_ring_op,
        unset_current_worker_ring,
    };

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("mududb-iou-create-dir-all")
            .join(format!("{name}-{}", crate::random::uuid_v4()))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    fn drive_iouring_future<F, T>(future: F) -> Option<T>
    where
        F: Future<Output = T>,
    {
        let mut ring = match IoUring::new(16) {
            Ok(ring) => ring,
            Err(err) => {
                eprintln!("skip iou_create_dir_all test: io_uring unavailable ({err})");
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

    #[test]
    fn iou_create_dir_all_succeeds_when_directory_exists() {
        let path = temp_path("exists").join("a").join("b");
        cleanup(&path);
        std::fs::create_dir_all(&path).unwrap();

        let result = drive_iouring_future(iou_create_dir_all(&path));
        if let Some(result) = result {
            result.unwrap();
            assert!(path.is_dir());
        }

        cleanup(&path);
    }

    #[test]
    fn iou_create_dir_all_succeeds_when_directory_does_not_exist() {
        let root = temp_path("missing");
        let path = root.join("a").join("b").join("c");
        cleanup(&root);

        let result = drive_iouring_future(iou_create_dir_all(&path));
        if let Some(result) = result {
            result.unwrap();
            assert!(path.is_dir());
        }

        cleanup(&root);
    }

    #[test]
    fn iou_create_dir_all_succeeds_when_parent_partially_exists() {
        let root = temp_path("partial");
        let existing = root.join("a");
        let path = existing.join("b").join("c");
        cleanup(&root);
        std::fs::create_dir_all(&existing).unwrap();

        let result = drive_iouring_future(iou_create_dir_all(&path));
        if let Some(result) = result {
            result.unwrap();
            assert!(existing.is_dir());
            assert!(path.is_dir());
        }

        cleanup(&root);
    }
}
