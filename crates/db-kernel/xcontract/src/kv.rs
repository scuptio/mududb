use super::*;

pub(crate) mod commit;
pub(crate) mod kv_ops;
pub(crate) mod recovery;
mod rpc_register;
pub(crate) mod tx;

/// Clones the worker's WAL log backend handle, when one is installed.
pub(crate) fn log_cloned(
    log: &SMutex<Option<ChunkedWorkerLogBackend>>,
) -> RS<Option<ChunkedWorkerLogBackend>> {
    let guard = log.lock()?;
    Ok(guard.clone())
}
