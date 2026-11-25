//! Complete durable install intents before workers open any storage.
use crate::server_launch::ServerLaunch;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::task_async::CurrentThreadTaskRuntime;

pub(super) fn recover(launch: &ServerLaunch) -> RS<()> {
    let cfg = launch.cfg();
    if ::repl::snapshot::ensure_no_pending_installs(cfg.data_dir()).is_ok() {
        return Ok(());
    }
    let runtime = CurrentThreadTaskRuntime::new()
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "build snapshot recovery runtime", e))?;
    let data_dir = cfg.data_dir().to_owned();
    let log_dir = cfg.log_dir().to_owned();
    let chunk_size = cfg.log_chunk_size();
    let registry = launch.deps().worker_registry();
    runtime.block_on(async move {
        ::repl::snapshot::recover_pending_installs(
            &data_dir,
            &log_dir,
            chunk_size,
            registry.workers(),
        )
        .await
    })
}
