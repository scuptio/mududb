use std::thread;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

pub fn run_async<F, T>(future: F) -> RS<F::Output>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let thread = thread::Builder::new().spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move { future.await })
    });

    let result = thread.map_err(|e| m_error!(EC::InternalErr, "join thread error", e))?;
    let r = result
        .join()
        .map_err(|_e| m_error!(EC::InternalErr, "join thread error"))?;
    Ok(r)
}