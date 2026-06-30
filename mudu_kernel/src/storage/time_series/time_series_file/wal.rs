use super::io::{ensure_time_series_file_exists_async, page_offset};
use super::{TimeSeriesFile, TimeSeriesFileIdentity};
use crate::wal::lsn::LSN;
use crate::wal::pl_batch::{
    new_pl_batch_worker_log, new_pl_batch_writer, NoopPLBatchRecoveryHandler, PLBatch,
};
use crate::wal::pl_entry::{PLEntry, PLFileId, PLOp};
use crate::wal::typed_worker_log::AsyncWorkerLogRecoveryHandler;
use crate::wal::worker_log::AsyncWorkerLogRecoverySource;
use crate::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogLayout};
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::default_sys_io_context;
use mudu_utils::scoped_task_trace;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) async fn new_relation_wal_backend(
    base_path: &Path,
    identity: &TimeSeriesFileIdentity,
) -> RS<ChunkedWorkerLogBackend> {
    new_relation_wal_backend_with_provider(
        base_path,
        identity,
        default_sys_io_context().provider_arc(),
    )
    .await
}

pub(super) async fn new_relation_wal_backend_with_provider(
    base_path: &Path,
    identity: &TimeSeriesFileIdentity,
    provider: Arc<dyn AsyncIoProvider>,
) -> RS<ChunkedWorkerLogBackend> {
    scoped_task_trace!();
    // Each relation file gets its own physical-log stream so recovery can
    // replay one file independently of the rest of the worker state.
    let log_dir = base_path.join("relation_wal");
    let layout = WorkerLogLayout::new(
        log_dir,
        time_series_log_oid(identity),
        super::RELATION_WAL_CHUNK_SIZE,
    )?;
    ChunkedWorkerLogBackend::new_direct_with_provider(layout, provider).await
}

fn time_series_log_oid(identity: &TimeSeriesFileIdentity) -> OID {
    identity.partition_id.rotate_left(17)
        ^ identity.table_id.rotate_left(53)
        ^ (identity.file_index as u128).rotate_left(97)
        ^ 0x706c5f74735f66696c655f77616c_u128
}

pub(super) async fn recover_relation_file(
    base_path: &Path,
    identity: &TimeSeriesFileIdentity,
    backend: &ChunkedWorkerLogBackend,
) -> RS<()> {
    recover_relation_file_async(default_sys_io_context().fs(), base_path, identity, backend).await
}

pub(super) async fn recover_relation_file_async(
    fs: Arc<dyn AsyncFs>,
    base_path: &Path,
    identity: &TimeSeriesFileIdentity,
    backend: &ChunkedWorkerLogBackend,
) -> RS<()> {
    scoped_task_trace!();
    let mut source = RelationWalRecoverySource {
        fs: fs.clone(),
        backend: backend.clone(),
    };
    let handler = Arc::new(RelationWalRecoveryHandler {
        fs,
        path: TimeSeriesFile::relation_file_path(
            base_path,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        ),
        file_id: PLFileId {
            partition_id: identity.partition_id,
            table_id: identity.table_id,
            file_index: identity.file_index,
        },
    });
    let log = new_pl_batch_worker_log(backend.clone(), NoopPLBatchRecoveryHandler);
    log.recover_async_with_handler(&mut source, &handler).await
}

async fn apply_recovered_entry_async(fs: &dyn AsyncFs, path: &Path, entry: &PLEntry) -> RS<()> {
    for op in &entry.ops {
        match op {
            PLOp::Create => ensure_time_series_file_exists_async(fs, path).await?,
            PLOp::Delete => fs.remove_file_if_exists(path).await?,
            PLOp::PageUpdate(update) => {
                ensure_time_series_file_exists_async(fs, path).await?;
                let file = fs.open(path, FileOptions::read_write_create()).await?;
                file.write_all_at(
                    page_offset(update.page_id)? + update.offset as u64,
                    &update.data,
                )
                .await?;
            }
        }
    }
    Ok(())
}

struct RelationWalRecoverySource {
    fs: Arc<dyn AsyncFs>,
    backend: ChunkedWorkerLogBackend,
}

#[async_trait]
impl AsyncWorkerLogRecoverySource for RelationWalRecoverySource {
    async fn chunk_paths_sorted(&mut self) -> RS<Vec<PathBuf>> {
        scoped_task_trace!();
        self.backend.chunk_paths_sorted().await
    }

    async fn read_chunk(&mut self, path: &Path) -> RS<Vec<u8>> {
        self.fs.as_ref().read_all(path).await
    }
}

struct RelationWalRecoveryHandler {
    fs: Arc<dyn AsyncFs>,
    path: PathBuf,
    file_id: PLFileId,
}

#[async_trait]
impl AsyncWorkerLogRecoveryHandler<PLBatch> for Arc<RelationWalRecoveryHandler> {
    async fn handle_entry(&self, entry: PLBatch, _start_lsn: LSN) -> RS<()> {
        scoped_task_trace!();
        for item in &entry.entries {
            if item.file != self.file_id {
                continue;
            }
            apply_recovered_entry_async(self.fs.as_ref(), &self.path, item).await?;
        }
        Ok(())
    }
}

pub(super) async fn append_file_create_async(
    backend: &ChunkedWorkerLogBackend,
    identity: &TimeSeriesFileIdentity,
) -> RS<()> {
    let writer = new_pl_batch_writer(backend.clone());
    writer
        .append(&PLBatch::new(vec![PLEntry {
            file: PLFileId {
                partition_id: identity.partition_id,
                table_id: identity.table_id,
                file_index: identity.file_index,
            },
            ops: vec![PLOp::Create],
        }]))
        .await?;
    Ok(())
}
