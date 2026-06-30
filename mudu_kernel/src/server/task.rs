#[cfg(target_os = "linux")]
use mudu::common::result::RS;
#[cfg(target_os = "linux")]
use mudu_sys::io::worker_ring::with_current_ring;
#[cfg(target_os = "linux")]
use mudu_utils::task_context::TaskContext;
#[cfg(target_os = "linux")]
use mudu_utils::task_id::new_task_id;

#[cfg(target_os = "linux")]
pub fn spawn(conn_id: Option<u64>, future: impl std::future::Future<Output = RS<()>> + 'static) {
    if !mudu_sys::io::worker_ring::has_current_worker_ring() {
        return;
    }
    let future = Box::pin(future);
    let _ = with_current_ring(|ring| {
        let trace_task_id = new_task_id();
        let task_name = match conn_id {
            Some(conn_id) => format!("iouring-task-conn-{conn_id}"),
            None => "iouring-task".to_string(),
        };
        let _ = TaskContext::new_iouring_context(trace_task_id, task_name);
        ring.worker_task_registry()
            .spawn_with_trace_id(conn_id, trace_task_id, future);
        Ok(())
    });
}

#[cfg(target_os = "linux")]
pub fn spawn_system(name: &str, future: impl std::future::Future<Output = RS<()>> + 'static) {
    if !mudu_sys::io::worker_ring::has_current_worker_ring() {
        return;
    }
    let future = Box::pin(future);
    let _ = with_current_ring(|ring| {
        let trace_task_id = new_task_id();
        let _ = TaskContext::new_iouring_context(trace_task_id, name.to_string());
        ring.worker_task_registry()
            .spawn_with_trace_id(None, trace_task_id, future);
        Ok(())
    });
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn spawn<T>(_conn_id: Option<u64>, _future: T) {}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn spawn_system<T>(_name: &str, _future: T) {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use super::{spawn, spawn_system};

    #[test]
    fn spawn_does_not_poll_future_without_worker_ring() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_moved = flag.clone();
        let future = async move {
            flag_moved.store(true, Ordering::SeqCst);
            Ok(())
        };
        spawn(Some(1), future);
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn spawn_system_does_not_poll_future_without_worker_ring() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_moved = flag.clone();
        let future = async move {
            flag_moved.store(true, Ordering::SeqCst);
            Ok(())
        };
        spawn_system("test", future);
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn has_current_worker_ring_is_false_and_call_does_not_panic() {
        assert!(!mudu_sys::io::worker_ring::has_current_worker_ring());
        spawn(Some(1), async { Ok(()) });
        spawn_system("test", async { Ok(()) });
    }
}
