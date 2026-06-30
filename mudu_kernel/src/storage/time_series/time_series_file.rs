use crate::storage::page::PageId;
use crate::wal::pl_batch::{new_pl_batch_writer, PLBatch};
use crate::wal::pl_entry::{PLEntry, PLFileId, PLOp};
use crate::wal::worker_log::ChunkedWorkerLogBackend;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::fs::SysFile;
use scc::HashMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FILE_MODE_644: u32 = 0o644;
const RELATION_WAL_CHUNK_SIZE: u64 = 256 * 1024;

/// Logical identity for one physical time-series file.
///
/// The relation layer assigns `file_index` values and WAL only works with this
/// numeric identity, never with `"key"` / `"value"` strings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesFileIdentity {
    pub partition_id: OID,
    pub table_id: OID,
    pub file_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeSeriesRecord {
    pub timestamp: u64,
    pub tuple_id: u64,
    pub payload: Vec<u8>,
    pub page_id: PageId,
    pub slot_index: usize,
}

pub struct TimeSeriesFile {
    // Relation-owned files carry a stable identity and a dedicated PL backend.
    // Standalone test files leave both fields as `None`.
    fs: Option<Arc<dyn AsyncFs>>,
    identity: Option<TimeSeriesFileIdentity>,
    path: PathBuf,
    file: Option<SysFile>,
    wal_backend: Option<ChunkedWorkerLogBackend>,
    page_cache: HashMap<PageId, Vec<u8>>,
    page_count: PageId,
    head_page_id: Option<PageId>,
    tail_page_id: Option<PageId>,
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u64,
}

impl TimeSeriesFile {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity(&self) -> Option<&TimeSeriesFileIdentity> {
        self.identity.as_ref()
    }

    pub fn page_count(&self) -> PageId {
        self.page_count
    }

    pub fn head_page_id(&self) -> Option<PageId> {
        self.head_page_id
    }

    pub fn tail_page_id(&self) -> Option<PageId> {
        self.tail_page_id
    }

    fn file_ref(&self) -> RS<&SysFile> {
        self.file
            .as_ref()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "time series file is not open"))
    }

    fn take_file(&mut self) -> RS<SysFile> {
        self.file
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "time series file is not open"))
    }

    pub async fn flush(&self) -> RS<()> {
        io::flush_file(self.file_ref()?).await
    }

    pub async fn close(mut self) -> RS<()> {
        io::close_file(self.take_file()?).await
    }

    pub fn close_sync(mut self) -> RS<()> {
        drop(self.take_file()?);
        Ok(())
    }

    pub async fn delete_file(mut self) -> RS<()> {
        if let Some(identity) = self.identity.as_ref() {
            let backend = self.wal_backend.clone().ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "missing time series wal backend")
            })?;
            let writer = new_pl_batch_writer(backend);
            writer
                .append(&PLBatch::new(vec![PLEntry {
                    file: PLFileId {
                        partition_id: identity.partition_id,
                        table_id: identity.table_id,
                        file_index: identity.file_index,
                    },
                    ops: vec![PLOp::Delete],
                }]))
                .await?;
        }
        io::close_file(self.take_file()?).await?;
        match self.fs.as_ref() {
            Some(fs) => fs.remove_file_if_exists(&self.path).await,
            None => io::remove_file_if_exists_async(&self.path).await,
        }
    }
}

mod io;
mod open;
mod page;
mod plan;
mod read;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
mod wal;
mod write;
