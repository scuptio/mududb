#[cfg(target_os = "linux")]
use mudu_sys::io::worker_ring::with_current_ring;
#[cfg(target_os = "linux")]
use mudu_sys::server::worker_task::WorkerTaskFuture;
#[cfg(target_os = "linux")]
use mudu_utils::task_context::TaskContext;
#[cfg(target_os = "linux")]
use mudu_utils::task_id::new_task_id;

#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn spawn(conn_id: Option<u64>, future: WorkerTaskFuture) {
    if !mudu_sys::io::worker_ring::has_current_worker_ring() {
        return;
    }
    let _ = with_current_ring(|ring| {
        let trace_task_id = new_task_id();
        let task_name = match conn_id {
            Some(conn_id) => format!("iouring-task-conn-{conn_id}"),
            None => "iouring-task".to_string(),
        };
        let _ = TaskContext::new_context(trace_task_id, task_name, false);
        ring.worker_task_registry()
            .spawn_with_trace_id(conn_id, trace_task_id, future);
        Ok(())
    });
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn spawn_system(name: &str, future: WorkerTaskFuture) {
    if !mudu_sys::io::worker_ring::has_current_worker_ring() {
        return;
    }
    let _ = with_current_ring(|ring| {
        let trace_task_id = new_task_id();
        let _ = TaskContext::new_context(trace_task_id, name.to_string(), false);
        ring.worker_task_registry()
            .spawn_with_trace_id(None, trace_task_id, future);
        Ok(())
    });
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn spawn<T>(_conn_id: Option<u64>, _future: T)
where
    T: Send + 'static,
{
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn spawn_system<T>(_name: &str, _future: T)
where
    T: Send + 'static,
{
}
