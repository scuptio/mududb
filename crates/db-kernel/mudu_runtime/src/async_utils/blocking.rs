use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::task_async::build_multi_thread_runtime;
use std::future::Future;

/// Run an async future on a dedicated thread and block until it completes.
pub fn run_async<F, T>(future: F) -> RS<F::Output>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let thread = mudu_sys::task::sync::spawn_thread(move || -> RS<F::Output> {
        let runtime = build_multi_thread_runtime()?;
        Ok(runtime.block_on(future))
    })?;
    let r = thread
        .join()
        .map_err(|_e| mudu_error!(ErrorCode::Internal, "join thread error"))??;
    Ok(r)
}
