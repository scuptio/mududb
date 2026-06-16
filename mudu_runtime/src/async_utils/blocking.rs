use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::task_async::build_multi_thread_runtime;
use std::future::Future;
pub fn run_async<F, T>(future: F) -> RS<F::Output>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let thread = mudu_sys::task::sync::spawn_thread(move || {
        let runtime = build_multi_thread_runtime().unwrap();
        runtime.block_on(future)
    })?;
    let r = thread
        .join()
        .map_err(|_e| m_error!(EC::InternalErr, "join thread error"))?;
    Ok(r)
}
