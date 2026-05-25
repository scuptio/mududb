use crate::async_rt::contract::{AsyncFs, FileOpenOptions};
use crate::async_rt::tokio::fs::TokioFs;
use crate::io::file;
use crate::io::file::IoFile;
use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::storage::page::page_block_ref_mut::PageBlockRefMut;
use crate::storage::page::page_header::NONE_PAGE_ID;
use crate::storage::page::PageId;
use crate::wal::pl_batch::{
    new_pl_batch_worker_log, new_pl_batch_writer, NoopPLBatchRecoveryHandler, PLBatch,
};
use crate::wal::pl_entry::{PLEntry, PLFileId, PLOp, PageUpdate};
use crate::wal::typed_worker_log::AsyncWorkerLogRecoveryHandler;
use crate::wal::worker_log::AsyncWorkerLogRecoverySource;
use crate::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogLayout};
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use scc::HashMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::trace;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageInsertLocation {
    Existing(PageId),
    Before(PageId),
    After(PageId),
    EmptyFile,
}

pub struct TimeSeriesFile {
    // Relation-owned files carry a stable identity and a dedicated PL backend.
    // Standalone test files leave both fields as `None`.
    fs: Option<Arc<dyn AsyncFs>>,
    identity: Option<TimeSeriesFileIdentity>,
    path: PathBuf,
    file: IoFile,
    wal_backend: Option<ChunkedWorkerLogBackend>,
    page_cache: HashMap<PageId, Vec<u8>>,
    page_count: PageId,
    head_page_id: Option<PageId>,
    tail_page_id: Option<PageId>,
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u32,
}

#[derive(Clone)]
struct PlannedPageWrite {
    page_id: PageId,
    image: Vec<u8>,
}

// A complete physical mutation to one file. The write path first builds this
// in memory, persists it as PL, and only then applies the page images.
#[derive(Clone, Default)]
struct TimeSeriesFileMutationPlan {
    create_file: bool,
    delete_file: bool,
    page_writes: Vec<PlannedPageWrite>,
    next_page_count: Option<PageId>,
    next_head_page_id: Option<Option<PageId>>,
    next_tail_page_id: Option<Option<PageId>>,
}

impl TimeSeriesFile {
    pub fn relation_file_path<P: AsRef<Path>>(
        base_path: P,
        partition_id: OID,
        table_id: OID,
        file_index: u32,
    ) -> PathBuf {
        let mut path_buf = base_path.as_ref().to_path_buf();
        path_buf.push("relation");
        path_buf.push(format!("{partition_id}.{table_id}.{file_index}.dat"));
        path_buf
    }

    /// Opens a relation-owned time-series file and replays its dedicated PL
    /// stream before any file state is observed.
    pub async fn open_relation_file<P: AsRef<Path>>(
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        Self::open_relation_file_with_fs(
            Arc::new(TokioFs::new()),
            base_path,
            identity,
            tuple_schema_hash,
            create_if_missing,
        )
        .await
    }

    /// Async relation-file open path with explicit file-system backend.
    pub async fn open_relation_file_with_fs<P: AsRef<Path>>(
        fs: Arc<dyn AsyncFs>,
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        trace!(
            table_id = identity.table_id,
            partition_id = identity.partition_id,
            file_index = identity.file_index,
            create_if_missing,
            "time_series open_relation_file_with_fs start"
        );
        let base_path = base_path.as_ref().to_path_buf();
        let path = Self::relation_file_path(
            &base_path,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );
        let wal_backend = new_relation_wal_backend(&base_path, &identity)?;
        trace!(path = %path.display(), "time_series recovering relation file");
        recover_relation_file_async(fs.clone(), &base_path, &identity, &wal_backend).await?;
        if create_if_missing && !fs.path_exists(&path).await? {
            trace!(path = %path.display(), "time_series appending create-file wal record");
            append_file_create_async(&wal_backend, &identity).await?;
        }
        trace!(path = %path.display(), "time_series opening relation file inner");
        Self::open_inner_with_fs(
            fs,
            path,
            Some(identity),
            Some(wal_backend),
            tuple_schema_hash,
            create_if_missing,
        )
        .await
    }

    /// Sync version of [`TimeSeriesFile::open_relation_file`].
    pub fn open_relation_file_sync<P: AsRef<Path>>(
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        let path = Self::relation_file_path(
            &base_path,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );
        let wal_backend = new_relation_wal_backend(&base_path, &identity)?;
        recover_relation_file(&base_path, &identity, &wal_backend)?;
        if create_if_missing && !path.exists() {
            append_file_create_sync(&wal_backend, &identity)?;
        }
        Self::open_inner_sync(
            path,
            Some(identity),
            Some(wal_backend),
            tuple_schema_hash,
            create_if_missing,
        )
    }

    pub async fn open_ts_file<P: AsRef<Path>>(path: P, create_if_missing: bool) -> RS<Self> {
        Self::open_ts_file_with_fs(Arc::new(TokioFs::new()), path, create_if_missing).await
    }

    pub async fn open_ts_file_with_fs<P: AsRef<Path>>(
        fs: Arc<dyn AsyncFs>,
        path: P,
        create_if_missing: bool,
    ) -> RS<Self> {
        Self::open_inner_with_fs(
            fs,
            path.as_ref().to_path_buf(),
            None,
            None,
            0,
            create_if_missing,
        )
        .await
    }

    pub fn open_ts_file_sync<P: AsRef<Path>>(path: P, create_if_missing: bool) -> RS<Self> {
        Self::open_inner_sync(
            path.as_ref().to_path_buf(),
            None,
            None,
            0,
            create_if_missing,
        )
    }

    async fn open_inner_with_fs(
        fs: Arc<dyn AsyncFs>,
        path: PathBuf,
        identity: Option<TimeSeriesFileIdentity>,
        wal_backend: Option<ChunkedWorkerLogBackend>,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        let path = path.to_path_buf();
        if let Some(parent) = path.parent() {
            trace!(path = %path.display(), parent = %parent.display(), "time_series ensuring parent dir");
            fs.create_dir_all(parent).await?;
        }

        let flags = if create_if_missing {
            libc::O_CREAT | libc::O_RDWR | file::cloexec_flag()
        } else {
            libc::O_RDWR | file::cloexec_flag()
        };
        trace!(path = %path.display(), flags, "time_series opening rw file");
        let file = open_rw(&path, flags).await?;
        trace!(path = %path.display(), "time_series opened rw file, reading metadata len by fd");
        let len = file::metadata_len_by_file(&file)?;
        if len % PAGE_SIZE as u64 != 0 {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "time series file length {} is not aligned to page size {}",
                    len, PAGE_SIZE
                )
            ));
        }

        let page_count = (len / PAGE_SIZE as u64) as u32;
        let (head_page_id, tail_page_id) =
            load_chain_metadata(&file, page_count, tuple_schema_hash).await?;
        Ok(Self {
            fs: Some(fs),
            identity,
            path,
            file,
            wal_backend,
            page_cache: HashMap::new(),
            page_count,
            head_page_id,
            tail_page_id,
            tuple_format_version: if tuple_schema_hash != 0 { 1 } else { 0 },
            tuple_schema_hash,
            tuple_flags: 0,
        })
    }

    fn open_inner_sync(
        path: PathBuf,
        identity: Option<TimeSeriesFileIdentity>,
        wal_backend: Option<ChunkedWorkerLogBackend>,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        let path = path.to_path_buf();
        if let Some(parent) = path.parent() {
            mudu_sys::fs::create_dir_all(parent)
                .map_err(|e| m_error!(EC::IOErr, "create time series dir error", e))?;
        }

        let flags = if create_if_missing {
            libc::O_CREAT | libc::O_RDWR | file::cloexec_flag()
        } else {
            libc::O_RDWR | file::cloexec_flag()
        };
        let file = open_rw_sync(&path, flags)?;
        let len = mudu_sys::fs::metadata_len(&path)
            .map_err(|e| m_error!(EC::IOErr, "read time series file metadata error", e))?;
        if len % PAGE_SIZE as u64 != 0 {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "time series file length {} is not aligned to page size {}",
                    len, PAGE_SIZE
                )
            ));
        }

        let page_count = (len / PAGE_SIZE as u64) as u32;
        let (head_page_id, tail_page_id) =
            load_chain_metadata_sync(&file, page_count, tuple_schema_hash)?;
        Ok(Self {
            fs: None,
            identity,
            path,
            file,
            wal_backend,
            page_cache: HashMap::new(),
            page_count,
            head_page_id,
            tail_page_id,
            tuple_format_version: if tuple_schema_hash != 0 { 1 } else { 0 },
            tuple_schema_hash,
            tuple_flags: 0,
        })
    }

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

    pub async fn flush(&self) -> RS<()> {
        flush_file(&self.file).await
    }

    pub async fn close(self) -> RS<()> {
        close_file(self.file).await
    }

    pub fn close_sync(self) -> RS<()> {
        file::close_sync(self.file)
    }

    pub async fn delete_file(mut self) -> RS<()> {
        if let Some(identity) = self.identity.as_ref() {
            let backend = self
                .wal_backend
                .clone()
                .ok_or_else(|| m_error!(EC::InternalErr, "missing time series wal backend"))?;
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
        close_file(std::mem::take(&mut self.file)).await?;
        match self.fs.as_ref() {
            Some(fs) => fs.remove_file_if_exists(&self.path).await,
            None => remove_file_if_exists(&self.path),
        }
    }

    pub fn delete_file_sync(mut self) -> RS<()> {
        if let Some(identity) = self.identity.as_ref() {
            let backend = self
                .wal_backend
                .clone()
                .ok_or_else(|| m_error!(EC::InternalErr, "missing time series wal backend"))?;
            let writer = new_pl_batch_writer(backend);
            writer.append_sync(&PLBatch::new(vec![PLEntry {
                file: PLFileId {
                    partition_id: identity.partition_id,
                    table_id: identity.table_id,
                    file_index: identity.file_index,
                },
                ops: vec![PLOp::Delete],
            }]))?;
        }
        file::close_sync(std::mem::take(&mut self.file))?;
        remove_file_if_exists(&self.path)
    }

    pub async fn get(&self, timestamp: u64, tuple_id: u64) -> RS<Option<TimeSeriesRecord>> {
        let mut current = self.head_page_id;
        while let Some(page_id) = current {
            let page_buf = self.read_page(page_id).await?;
            let page = PageBlockRef::new(&page_buf);
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if timestamp > max_ts {
                    return Ok(None);
                }
                if timestamp < min_ts {
                    current = page.active_next_page()?;
                    continue;
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    return Ok(Some(TimeSeriesRecord {
                        timestamp,
                        tuple_id,
                        payload: page.record_bytes(slot_index)?.to_vec(),
                        page_id,
                        slot_index,
                    }));
                }
            }
            current = page.active_next_page()?;
        }
        Ok(None)
    }

    pub fn get_sync(&self, timestamp: u64, tuple_id: u64) -> RS<Option<TimeSeriesRecord>> {
        let mut current = self.head_page_id;
        while let Some(page_id) = current {
            let page_buf = self.read_page_sync(page_id)?;
            let page = PageBlockRef::new(&page_buf);
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if timestamp > max_ts {
                    return Ok(None);
                }
                if timestamp < min_ts {
                    current = page.active_next_page()?;
                    continue;
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    return Ok(Some(TimeSeriesRecord {
                        timestamp,
                        tuple_id,
                        payload: page.record_bytes(slot_index)?.to_vec(),
                        page_id,
                        slot_index,
                    }));
                }
            }
            current = page.active_next_page()?;
        }
        Ok(None)
    }

    pub async fn scan_range(&self, begin_ts: u64, end_ts: u64) -> RS<Vec<TimeSeriesRecord>> {
        if begin_ts > end_ts {
            return Ok(vec![]);
        }

        let mut current = self.head_page_id;
        let mut rows = vec![];
        while let Some(page_id) = current {
            let page_buf = self.read_page(page_id).await?;
            let page = PageBlockRef::new(&page_buf);
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if max_ts < begin_ts {
                    break;
                }
                if min_ts <= end_ts && max_ts >= begin_ts {
                    let count = page.slot_count()?;
                    for slot_index in 0..count {
                        let slot = page.slot_ref(slot_index)?;
                        let ts = slot.timestamp();
                        if ts < begin_ts || ts > end_ts {
                            continue;
                        }
                        rows.push(TimeSeriesRecord {
                            timestamp: ts,
                            tuple_id: slot.tuple_id(),
                            payload: page.record_bytes(slot_index)?.to_vec(),
                            page_id,
                            slot_index,
                        });
                    }
                }
            }
            current = page.active_next_page()?;
        }

        rows.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.tuple_id.cmp(&right.tuple_id))
        });
        Ok(rows)
    }

    pub fn scan_range_sync(&self, begin_ts: u64, end_ts: u64) -> RS<Vec<TimeSeriesRecord>> {
        if begin_ts > end_ts {
            return Ok(vec![]);
        }

        let mut current = self.head_page_id;
        let mut rows = vec![];
        while let Some(page_id) = current {
            let page_buf = self.read_page_sync(page_id)?;
            let page = PageBlockRef::new(&page_buf);
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if max_ts < begin_ts {
                    break;
                }
                if min_ts <= end_ts && max_ts >= begin_ts {
                    let count = page.slot_count()?;
                    for slot_index in 0..count {
                        let slot = page.slot_ref(slot_index)?;
                        let ts = slot.timestamp();
                        if ts < begin_ts || ts > end_ts {
                            continue;
                        }
                        rows.push(TimeSeriesRecord {
                            timestamp: ts,
                            tuple_id: slot.tuple_id(),
                            payload: page.record_bytes(slot_index)?.to_vec(),
                            page_id,
                            slot_index,
                        });
                    }
                }
            }
            current = page.active_next_page()?;
        }

        rows.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.tuple_id.cmp(&right.tuple_id))
        });
        Ok(rows)
    }

    pub async fn insert(&mut self, timestamp: u64, tuple_id: u64, payload: &[u8]) -> RS<()> {
        match self.find_insert_location(timestamp).await? {
            PageInsertLocation::EmptyFile => {
                let page_id = self.page_count;
                let mut page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut page_buf);
                    page.insert_record(timestamp, tuple_id, payload)?;
                }
                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: page_buf,
                });
                plan.next_page_count = Some(page_id + 1);
                plan.next_head_page_id = Some(Some(page_id));
                plan.next_tail_page_id = Some(Some(page_id));
                self.persist_plan(plan).await?;
            }
            PageInsertLocation::Existing(page_id) => {
                let page_buf = self.read_page(page_id).await?;
                let page = PageBlockRef::new(&page_buf);
                if self.tuple_schema_hash != 0 {
                    let header = page.header()?;
                    if header.tuple_schema_hash() != self.tuple_schema_hash {
                        return Err(m_error!(
                            EC::DecodeErr,
                            format!(
                                "tuple schema hash mismatch on page {}: expected {} got {}",
                                page_id,
                                self.tuple_schema_hash,
                                header.tuple_schema_hash()
                            )
                        ));
                    }
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    self.update_in_page(page_id, slot_index, timestamp, tuple_id, payload)
                        .await?;
                    return Ok(());
                }

                let mut page_buf = page_buf;
                let insert_result = {
                    let mut page_mut = PageBlockRefMut::new(&mut page_buf);
                    page_mut.insert_record(timestamp, tuple_id, payload)
                };
                match insert_result {
                    Ok(_) => self.write_page(page_id, &page_buf).await?,
                    Err(err) if err.ec() == EC::InsufficientBufferSpace => {
                        self.split_insert_full_page(page_id, timestamp, tuple_id, payload)
                            .await?;
                    }
                    Err(err) => return Err(err),
                }
            }
            PageInsertLocation::Before(next_page_id) => {
                let page_id = self.page_count;
                let next_page_buf = self.read_page(next_page_id).await?;
                let next_page = PageBlockRef::new(&next_page_buf);
                let prev_page_id = next_page.active_prev_page()?;
                let mut new_page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut new_page_buf);
                    page.set_page_links(prev_page_id.unwrap_or(NONE_PAGE_ID), next_page_id)?;
                    page.insert_record(timestamp, tuple_id, payload)?;
                }

                let mut updated_next_buf = next_page_buf.clone();
                {
                    let header = PageBlockRef::new(&updated_next_buf).header()?;
                    let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                    page.set_page_links(page_id, header.next_page())?;
                }

                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: new_page_buf,
                });
                plan.page_writes.push(PlannedPageWrite {
                    page_id: next_page_id,
                    image: updated_next_buf,
                });
                if let Some(prev_page_id) = prev_page_id {
                    let prev_page_buf = self.read_page(prev_page_id).await?;
                    let mut updated_prev_buf = prev_page_buf.clone();
                    let header = PageBlockRef::new(&updated_prev_buf).header()?;
                    {
                        let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                        page.set_page_links(header.prev_page(), page_id)?;
                    }
                    plan.page_writes.push(PlannedPageWrite {
                        page_id: prev_page_id,
                        image: updated_prev_buf,
                    });
                } else {
                    plan.next_head_page_id = Some(Some(page_id));
                }
                plan.next_page_count = Some(page_id + 1);
                self.persist_plan(plan).await?;
            }
            PageInsertLocation::After(prev_page_id) => {
                let page_id = self.page_count;
                let prev_page_buf = self.read_page(prev_page_id).await?;
                let prev_page = PageBlockRef::new(&prev_page_buf);
                let next_page_id = prev_page.active_next_page()?;
                let mut new_page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut new_page_buf);
                    page.set_page_links(prev_page_id, next_page_id.unwrap_or(NONE_PAGE_ID))?;
                    page.insert_record(timestamp, tuple_id, payload)?;
                }

                let mut updated_prev_buf = prev_page_buf.clone();
                {
                    let header = PageBlockRef::new(&updated_prev_buf).header()?;
                    let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                    page.set_page_links(header.prev_page(), page_id)?;
                }

                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: new_page_buf,
                });
                plan.page_writes.push(PlannedPageWrite {
                    page_id: prev_page_id,
                    image: updated_prev_buf,
                });
                if let Some(next_page_id) = next_page_id {
                    let next_page_buf = self.read_page(next_page_id).await?;
                    let mut updated_next_buf = next_page_buf.clone();
                    let header = PageBlockRef::new(&updated_next_buf).header()?;
                    {
                        let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                        page.set_page_links(page_id, header.next_page())?;
                    }
                    plan.page_writes.push(PlannedPageWrite {
                        page_id: next_page_id,
                        image: updated_next_buf,
                    });
                } else {
                    plan.next_tail_page_id = Some(Some(page_id));
                }
                if self.head_page_id.is_none() {
                    plan.next_head_page_id = Some(Some(page_id));
                }
                plan.next_page_count = Some(page_id + 1);
                self.persist_plan(plan).await?;
            }
        }
        Ok(())
    }

    pub fn insert_sync(&mut self, timestamp: u64, tuple_id: u64, payload: &[u8]) -> RS<()> {
        match self.find_insert_location_sync(timestamp)? {
            PageInsertLocation::EmptyFile => {
                let page_id = self.page_count;
                let mut page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut page_buf);
                    page.insert_record(timestamp, tuple_id, payload)?;
                }
                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: page_buf,
                });
                plan.next_page_count = Some(page_id + 1);
                plan.next_head_page_id = Some(Some(page_id));
                plan.next_tail_page_id = Some(Some(page_id));
                self.persist_plan_sync(plan)?;
            }
            PageInsertLocation::Existing(page_id) => {
                let page_buf = self.read_page_sync(page_id)?;
                let page = PageBlockRef::new(&page_buf);
                if self.tuple_schema_hash != 0 {
                    let header = page.header()?;
                    if header.tuple_schema_hash() != self.tuple_schema_hash {
                        return Err(m_error!(
                            EC::DecodeErr,
                            format!(
                                "tuple schema hash mismatch on page {}: expected {} got {}",
                                page_id,
                                self.tuple_schema_hash,
                                header.tuple_schema_hash()
                            )
                        ));
                    }
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    self.update_in_page_sync(page_id, slot_index, timestamp, tuple_id, payload)?;
                    return Ok(());
                }

                let mut page_buf = page_buf;
                let insert_result = {
                    let mut page_mut = PageBlockRefMut::new(&mut page_buf);
                    page_mut.insert_record(timestamp, tuple_id, payload)
                };
                match insert_result {
                    Ok(_) => self.write_page_sync(page_id, &page_buf)?,
                    Err(err) if err.ec() == EC::InsufficientBufferSpace => {
                        self.split_insert_full_page_sync(page_id, timestamp, tuple_id, payload)?;
                    }
                    Err(err) => return Err(err),
                }
            }
            PageInsertLocation::Before(next_page_id) => {
                let page_id = self.page_count;
                let next_page_buf = self.read_page_sync(next_page_id)?;
                let next_page = PageBlockRef::new(&next_page_buf);
                let prev_page_id = next_page.active_prev_page()?;
                let mut new_page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut new_page_buf);
                    page.set_page_links(prev_page_id.unwrap_or(NONE_PAGE_ID), next_page_id)?;
                    page.insert_record(timestamp, tuple_id, payload)?;
                }

                let mut updated_next_buf = next_page_buf.clone();
                {
                    let header = PageBlockRef::new(&updated_next_buf).header()?;
                    let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                    page.set_page_links(page_id, header.next_page())?;
                }

                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: new_page_buf,
                });
                plan.page_writes.push(PlannedPageWrite {
                    page_id: next_page_id,
                    image: updated_next_buf,
                });
                if let Some(prev_page_id) = prev_page_id {
                    let prev_page_buf = self.read_page_sync(prev_page_id)?;
                    let mut updated_prev_buf = prev_page_buf.clone();
                    let header = PageBlockRef::new(&updated_prev_buf).header()?;
                    {
                        let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                        page.set_page_links(header.prev_page(), page_id)?;
                    }
                    plan.page_writes.push(PlannedPageWrite {
                        page_id: prev_page_id,
                        image: updated_prev_buf,
                    });
                } else {
                    plan.next_head_page_id = Some(Some(page_id));
                }
                plan.next_page_count = Some(page_id + 1);
                self.persist_plan_sync(plan)?;
            }
            PageInsertLocation::After(prev_page_id) => {
                let page_id = self.page_count;
                let prev_page_buf = self.read_page_sync(prev_page_id)?;
                let prev_page = PageBlockRef::new(&prev_page_buf);
                let next_page_id = prev_page.active_next_page()?;
                let mut new_page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut new_page_buf);
                    page.set_page_links(prev_page_id, next_page_id.unwrap_or(NONE_PAGE_ID))?;
                    page.insert_record(timestamp, tuple_id, payload)?;
                }

                let mut updated_prev_buf = prev_page_buf.clone();
                {
                    let header = PageBlockRef::new(&updated_prev_buf).header()?;
                    let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                    page.set_page_links(header.prev_page(), page_id)?;
                }

                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: new_page_buf,
                });
                plan.page_writes.push(PlannedPageWrite {
                    page_id: prev_page_id,
                    image: updated_prev_buf,
                });
                if let Some(next_page_id) = next_page_id {
                    let next_page_buf = self.read_page_sync(next_page_id)?;
                    let mut updated_next_buf = next_page_buf.clone();
                    let header = PageBlockRef::new(&updated_next_buf).header()?;
                    {
                        let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                        page.set_page_links(page_id, header.next_page())?;
                    }
                    plan.page_writes.push(PlannedPageWrite {
                        page_id: next_page_id,
                        image: updated_next_buf,
                    });
                } else {
                    plan.next_tail_page_id = Some(Some(page_id));
                }
                if self.head_page_id.is_none() {
                    plan.next_head_page_id = Some(Some(page_id));
                }
                plan.next_page_count = Some(page_id + 1);
                self.persist_plan_sync(plan)?;
            }
        }
        Ok(())
    }

    pub async fn delete(&mut self, timestamp: u64, tuple_id: u64) -> RS<bool> {
        let mut current = self.head_page_id;
        while let Some(page_id) = current {
            let page_buf = self.read_page(page_id).await?;
            let page = PageBlockRef::new(&page_buf);
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if timestamp > max_ts {
                    return Ok(false);
                }
                if timestamp < min_ts {
                    current = page.active_next_page()?;
                    continue;
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    let mut page_buf = page_buf;
                    {
                        let mut page_mut = PageBlockRefMut::new(&mut page_buf);
                        page_mut.delete_record(slot_index)?;
                    }
                    let mut plan = TimeSeriesFileMutationPlan::default();
                    plan.page_writes.push(PlannedPageWrite {
                        page_id,
                        image: page_buf,
                    });
                    self.persist_plan(plan).await?;
                    return Ok(true);
                }
            }
            current = page.active_next_page()?;
        }
        Ok(false)
    }

    pub fn delete_sync(&mut self, timestamp: u64, tuple_id: u64) -> RS<bool> {
        let mut current = self.head_page_id;
        while let Some(page_id) = current {
            let page_buf = self.read_page_sync(page_id)?;
            let page = PageBlockRef::new(&page_buf);
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if timestamp > max_ts {
                    return Ok(false);
                }
                if timestamp < min_ts {
                    current = page.active_next_page()?;
                    continue;
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    let mut page_buf = page_buf;
                    {
                        let mut page_mut = PageBlockRefMut::new(&mut page_buf);
                        page_mut.delete_record(slot_index)?;
                    }
                    let mut plan = TimeSeriesFileMutationPlan::default();
                    plan.page_writes.push(PlannedPageWrite {
                        page_id,
                        image: page_buf,
                    });
                    self.persist_plan_sync(plan)?;
                    return Ok(true);
                }
            }
            current = page.active_next_page()?;
        }
        Ok(false)
    }

    fn find_split_index(&self, entries: &[TimeSeriesRecord]) -> RS<usize> {
        for split_at in 1..entries.len() {
            if page_entries_fit(&entries[..split_at]) && page_entries_fit(&entries[split_at..]) {
                return Ok(split_at);
            }
        }
        Err(m_error!(
            EC::InsufficientBufferSpace,
            "records do not fit into two time series pages"
        ))
    }

    fn page_entries(&self, page: &PageBlockRef<'_>, page_id: PageId) -> RS<Vec<TimeSeriesRecord>> {
        let count = page.slot_count()?;
        let mut entries = Vec::with_capacity(count);
        for slot_index in 0..count {
            let slot = page.slot_ref(slot_index)?;
            entries.push(TimeSeriesRecord {
                timestamp: slot.timestamp(),
                tuple_id: slot.tuple_id(),
                payload: page.record_bytes(slot_index)?.to_vec(),
                page_id,
                slot_index,
            });
        }
        Ok(entries)
    }

    async fn read_page(&self, page_id: PageId) -> RS<Vec<u8>> {
        if page_id >= self.page_count {
            return Err(m_error!(
                EC::IndexOutOfRange,
                format!("page {} out of range {}", page_id, self.page_count)
            ));
        }
        if let Some(entry) = self.page_cache.get_sync(&page_id) {
            return Ok(entry.get().clone());
        }

        let page = read_file_exact(&self.file, PAGE_SIZE, page_offset(page_id)?).await?;
        let _ = self.page_cache.remove_sync(&page_id);
        let _ = self.page_cache.insert_sync(page_id, page.clone());
        Ok(page)
    }

    async fn update_in_page(
        &mut self,
        page_id: PageId,
        slot_index: usize,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let mut page_buf = self.read_page(page_id).await?;
        {
            let mut page_mut = PageBlockRefMut::new(&mut page_buf);
            page_mut.update_record(slot_index, timestamp, tuple_id, payload)?;
        }
        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: page_buf,
        });
        self.persist_plan(plan).await
    }

    async fn split_insert_full_page(
        &mut self,
        page_id: PageId,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let page_buf = self.read_page(page_id).await?;
        let page = PageBlockRef::new(&page_buf);
        let mut entries = self.page_entries(&page, page_id)?;
        entries.push(TimeSeriesRecord {
            timestamp,
            tuple_id,
            payload: payload.to_vec(),
            page_id,
            slot_index: 0,
        });
        entries.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.tuple_id.cmp(&right.tuple_id))
        });

        let split_at = self.find_split_index(&entries)?;
        let lower_entries = entries[..split_at].to_vec();
        let upper_entries = entries[split_at..].to_vec();

        let header = page.header()?;
        let old_next_page_id = page.active_next_page()?;
        let new_page_id = self.page_count;
        let current_page_buf = build_entries_page_image(
            page_id,
            header.prev_page(),
            new_page_id,
            &upper_entries,
            self.tuple_format_version,
            self.tuple_schema_hash,
            self.tuple_flags,
        )?;
        let new_page_buf = build_entries_page_image(
            new_page_id,
            page_id,
            old_next_page_id.unwrap_or(NONE_PAGE_ID),
            &lower_entries,
            self.tuple_format_version,
            self.tuple_schema_hash,
            self.tuple_flags,
        )?;

        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: current_page_buf,
        });
        plan.page_writes.push(PlannedPageWrite {
            page_id: new_page_id,
            image: new_page_buf,
        });
        if let Some(next_page_id) = old_next_page_id {
            let next_page_buf = self.read_page(next_page_id).await?;
            let mut updated_next_buf = next_page_buf.clone();
            let next_header = PageBlockRef::new(&updated_next_buf).header()?;
            {
                let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                page.set_page_links(new_page_id, next_header.next_page())?;
            }
            plan.page_writes.push(PlannedPageWrite {
                page_id: next_page_id,
                image: updated_next_buf,
            });
        } else {
            plan.next_tail_page_id = Some(Some(new_page_id));
        }
        plan.next_page_count = Some(new_page_id + 1);
        self.persist_plan(plan).await
    }

    async fn find_insert_location(&self, timestamp: u64) -> RS<PageInsertLocation> {
        let Some(mut current) = self.head_page_id else {
            return Ok(PageInsertLocation::EmptyFile);
        };

        let mut last_non_empty = None;
        loop {
            let page_buf = self.read_page(current).await?;
            let page = PageBlockRef::new(&page_buf);
            match page.timestamp_bounds()? {
                Some((min_ts, max_ts)) => {
                    last_non_empty = Some(current);
                    if timestamp > max_ts {
                        return Ok(PageInsertLocation::Before(current));
                    }
                    if timestamp >= min_ts {
                        return Ok(PageInsertLocation::Existing(current));
                    }
                }
                None => {}
            }

            match page.active_next_page()? {
                Some(next) => current = next,
                None => return Ok(PageInsertLocation::After(last_non_empty.unwrap_or(current))),
            }
        }
    }

    async fn write_page(&mut self, page_id: PageId, page: &[u8]) -> RS<()> {
        if page.len() != PAGE_SIZE {
            return Err(m_error!(
                EC::EncodeErr,
                format!(
                    "page write requires {} bytes, got {}",
                    PAGE_SIZE,
                    page.len()
                )
            ));
        }
        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: page.to_vec(),
        });
        self.persist_plan(plan).await
    }

    async fn persist_plan(&mut self, plan: TimeSeriesFileMutationPlan) -> RS<()> {
        // Physical WAL must reach durable storage before any data-page update.
        if let Some(batch) = self.build_pl_batch(&plan)? {
            trace!(
                path = %self.path.display(),
                create_file = plan.create_file,
                delete_file = plan.delete_file,
                page_writes = plan.page_writes.len(),
                "time_series persist_plan append wal"
            );
            let backend = self
                .wal_backend
                .clone()
                .ok_or_else(|| m_error!(EC::InternalErr, "missing time series wal backend"))?;
            let writer = new_pl_batch_writer(backend);
            writer.append(&batch).await?;
        }
        trace!(
            path = %self.path.display(),
            create_file = plan.create_file,
            delete_file = plan.delete_file,
            page_writes = plan.page_writes.len(),
            "time_series persist_plan apply plan"
        );
        self.apply_plan(&plan).await
    }

    async fn apply_plan(&mut self, plan: &TimeSeriesFileMutationPlan) -> RS<()> {
        if plan.create_file {
            trace!(path = %self.path.display(), "time_series apply_plan create file start");
            match self.fs.as_ref() {
                Some(fs) => ensure_time_series_file_exists_async(fs.as_ref(), &self.path).await?,
                None => ensure_time_series_file_exists_sync(&self.path)?,
            }
            trace!(path = %self.path.display(), "time_series apply_plan create file done");
        }
        for write in &plan.page_writes {
            trace!(path = %self.path.display(), page_id = write.page_id, "time_series apply_plan write page");
            self.apply_page_write(write.page_id, &write.image).await?;
        }
        if plan.delete_file {
            close_file(std::mem::take(&mut self.file)).await?;
            match self.fs.as_ref() {
                Some(fs) => fs.remove_file_if_exists(&self.path).await?,
                None => remove_file_if_exists(&self.path)?,
            }
            self.page_cache = HashMap::new();
        }
        if let Some(page_count) = plan.next_page_count {
            self.page_count = page_count;
        }
        if let Some(head_page_id) = plan.next_head_page_id {
            self.head_page_id = head_page_id;
        }
        if let Some(tail_page_id) = plan.next_tail_page_id {
            self.tail_page_id = tail_page_id;
        }
        Ok(())
    }

    async fn apply_page_write(&self, page_id: PageId, page: &[u8]) -> RS<()> {
        let _ = self.page_cache.remove_sync(&page_id);
        file::write(&self.file, page.to_vec(), page_offset(page_id)?)
            .await
            .map(|_| ())?;
        let _ = self.page_cache.insert_sync(page_id, page.to_vec());
        Ok(())
    }

    fn update_in_page_sync(
        &mut self,
        page_id: PageId,
        slot_index: usize,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let mut page_buf = self.read_page_sync(page_id)?;
        {
            let mut page_mut = PageBlockRefMut::new(&mut page_buf);
            page_mut.update_record(slot_index, timestamp, tuple_id, payload)?;
        }
        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: page_buf,
        });
        self.persist_plan_sync(plan)
    }

    fn split_insert_full_page_sync(
        &mut self,
        page_id: PageId,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let page_buf = self.read_page_sync(page_id)?;
        let page = PageBlockRef::new(&page_buf);
        let mut entries = self.page_entries(&page, page_id)?;
        entries.push(TimeSeriesRecord {
            timestamp,
            tuple_id,
            payload: payload.to_vec(),
            page_id,
            slot_index: 0,
        });
        entries.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.tuple_id.cmp(&right.tuple_id))
        });

        let split_at = self.find_split_index(&entries)?;
        let lower_entries = entries[..split_at].to_vec();
        let upper_entries = entries[split_at..].to_vec();

        let header = page.header()?;
        let old_next_page_id = page.active_next_page()?;
        let new_page_id = self.page_count;
        let current_page_buf = build_entries_page_image(
            page_id,
            header.prev_page(),
            new_page_id,
            &upper_entries,
            self.tuple_format_version,
            self.tuple_schema_hash,
            self.tuple_flags,
        )?;
        let new_page_buf = build_entries_page_image(
            new_page_id,
            page_id,
            old_next_page_id.unwrap_or(NONE_PAGE_ID),
            &lower_entries,
            self.tuple_format_version,
            self.tuple_schema_hash,
            self.tuple_flags,
        )?;

        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: current_page_buf,
        });
        plan.page_writes.push(PlannedPageWrite {
            page_id: new_page_id,
            image: new_page_buf,
        });
        if let Some(next_page_id) = old_next_page_id {
            let next_page_buf = self.read_page_sync(next_page_id)?;
            let mut updated_next_buf = next_page_buf.clone();
            let next_header = PageBlockRef::new(&updated_next_buf).header()?;
            {
                let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                page.set_page_links(new_page_id, next_header.next_page())?;
            }
            plan.page_writes.push(PlannedPageWrite {
                page_id: next_page_id,
                image: updated_next_buf,
            });
        } else {
            plan.next_tail_page_id = Some(Some(new_page_id));
        }
        plan.next_page_count = Some(new_page_id + 1);
        self.persist_plan_sync(plan)
    }

    fn find_insert_location_sync(&self, timestamp: u64) -> RS<PageInsertLocation> {
        let Some(mut current) = self.head_page_id else {
            return Ok(PageInsertLocation::EmptyFile);
        };

        let mut last_non_empty = None;
        loop {
            let page_buf = self.read_page_sync(current)?;
            let page = PageBlockRef::new(&page_buf);
            match page.timestamp_bounds()? {
                Some((min_ts, max_ts)) => {
                    last_non_empty = Some(current);
                    if timestamp > max_ts {
                        return Ok(PageInsertLocation::Before(current));
                    }
                    if timestamp >= min_ts {
                        return Ok(PageInsertLocation::Existing(current));
                    }
                }
                None => {}
            }

            match page.active_next_page()? {
                Some(next) => current = next,
                None => return Ok(PageInsertLocation::After(last_non_empty.unwrap_or(current))),
            }
        }
    }

    fn read_page_sync(&self, page_id: PageId) -> RS<Vec<u8>> {
        if page_id >= self.page_count {
            return Err(m_error!(
                EC::IndexOutOfRange,
                format!("page {} out of range {}", page_id, self.page_count)
            ));
        }
        if let Some(entry) = self.page_cache.get_sync(&page_id) {
            return Ok(entry.get().clone());
        }

        let page = read_file_exact_sync(&self.file, PAGE_SIZE, page_offset(page_id)?)?;
        let _ = self.page_cache.remove_sync(&page_id);
        let _ = self.page_cache.insert_sync(page_id, page.clone());
        Ok(page)
    }

    fn write_page_sync(&mut self, page_id: PageId, page: &[u8]) -> RS<()> {
        if page.len() != PAGE_SIZE {
            return Err(m_error!(
                EC::EncodeErr,
                format!(
                    "page write requires {} bytes, got {}",
                    PAGE_SIZE,
                    page.len()
                )
            ));
        }
        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: page.to_vec(),
        });
        self.persist_plan_sync(plan)
    }

    fn persist_plan_sync(&mut self, plan: TimeSeriesFileMutationPlan) -> RS<()> {
        // Physical WAL must reach durable storage before any data-page update.
        if let Some(batch) = self.build_pl_batch(&plan)? {
            let backend = self
                .wal_backend
                .clone()
                .ok_or_else(|| m_error!(EC::InternalErr, "missing time series wal backend"))?;
            let writer = new_pl_batch_writer(backend);
            writer.append_sync(&batch)?;
        }
        self.apply_plan_sync(&plan)
    }

    fn apply_plan_sync(&mut self, plan: &TimeSeriesFileMutationPlan) -> RS<()> {
        if plan.create_file {
            ensure_time_series_file_exists_sync(&self.path)?;
        }
        for write in &plan.page_writes {
            self.apply_page_write_sync(write.page_id, &write.image)?;
        }
        if plan.delete_file {
            file::close_sync(std::mem::take(&mut self.file))?;
            remove_file_if_exists(&self.path)?;
            self.page_cache = HashMap::new();
        }
        if let Some(page_count) = plan.next_page_count {
            self.page_count = page_count;
        }
        if let Some(head_page_id) = plan.next_head_page_id {
            self.head_page_id = head_page_id;
        }
        if let Some(tail_page_id) = plan.next_tail_page_id {
            self.tail_page_id = tail_page_id;
        }
        Ok(())
    }

    fn apply_page_write_sync(&self, page_id: PageId, page: &[u8]) -> RS<()> {
        let _ = self.page_cache.remove_sync(&page_id);
        write_file_all_sync(&self.file, page, page_offset(page_id)?)?;
        let _ = self.page_cache.insert_sync(page_id, page.to_vec());
        Ok(())
    }

    fn build_pl_batch(&self, plan: &TimeSeriesFileMutationPlan) -> RS<Option<PLBatch>> {
        let Some(identity) = self.identity.as_ref() else {
            return Ok(None);
        };
        let mut ops = Vec::new();
        if plan.create_file {
            ops.push(PLOp::Create);
        }
        for write in &plan.page_writes {
            if write.image.len() != PAGE_SIZE {
                return Err(m_error!(
                    EC::EncodeErr,
                    format!(
                        "page write requires {} bytes, got {}",
                        PAGE_SIZE,
                        write.image.len()
                    )
                ));
            }
            ops.push(PLOp::PageUpdate(PageUpdate {
                page_id: write.page_id,
                offset: 0,
                data: write.image.clone(),
            }));
        }
        if plan.delete_file {
            ops.push(PLOp::Delete);
        }
        if ops.is_empty() {
            return Ok(None);
        }
        Ok(Some(PLBatch::new(vec![PLEntry {
            file: PLFileId {
                partition_id: identity.partition_id,
                table_id: identity.table_id,
                file_index: identity.file_index,
            },
            ops,
        }])))
    }
}

fn build_entries_page_image(
    page_id: PageId,
    prev_page_id: PageId,
    next_page_id: PageId,
    entries: &[TimeSeriesRecord],
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u32,
) -> RS<Vec<u8>> {
    let mut page_buf = empty_page_image(
        page_id,
        tuple_format_version,
        tuple_schema_hash,
        tuple_flags,
    )?;
    {
        let mut page = PageBlockRefMut::new(&mut page_buf);
        page.set_page_links(prev_page_id, next_page_id)?;
    }
    for entry in entries {
        let mut page = PageBlockRefMut::new(&mut page_buf);
        page.insert_record(entry.timestamp, entry.tuple_id, &entry.payload)?;
    }
    Ok(page_buf)
}

fn empty_page_image(
    page_id: PageId,
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u32,
) -> RS<Vec<u8>> {
    let mut page_buf = vec![0u8; PAGE_SIZE];
    {
        let mut page = PageBlockRefMut::new(&mut page_buf);
        page.init_empty_with_tuple_meta(
            page_id,
            tuple_format_version,
            tuple_schema_hash,
            tuple_flags,
        )?;
    }
    Ok(page_buf)
}

fn page_entries_fit(entries: &[TimeSeriesRecord]) -> bool {
    let mut buf = vec![0u8; PAGE_SIZE];
    let mut page = PageBlockRefMut::new(&mut buf);
    if page.init_empty(0).is_err() {
        return false;
    }
    for entry in entries {
        if page
            .insert_record(entry.timestamp, entry.tuple_id, &entry.payload)
            .is_err()
        {
            return false;
        }
    }
    true
}

async fn load_chain_metadata(
    file: &IoFile,
    page_count: PageId,
    expected_schema_hash: u64,
) -> RS<(Option<PageId>, Option<PageId>)> {
    if page_count == 0 {
        return Ok((None, None));
    }

    let mut headers = Vec::with_capacity(page_count as usize);
    for page_id in 0..page_count {
        let buf = read_file_exact(file, PAGE_SIZE, page_offset(page_id)?).await?;
        let page = PageBlockRef::new(&buf);
        page.validate_layout()?;
        let header = page.header()?;
        if expected_schema_hash != 0 {
            if header.tuple_format_version() == 0 {
                return Err(m_error!(
                    EC::DecodeErr,
                    "missing tuple format version in page header"
                ));
            }
            if header.tuple_schema_hash() != expected_schema_hash {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!(
                        "page tuple schema hash mismatch: page_id={} expected={} got={}",
                        page_id,
                        expected_schema_hash,
                        header.tuple_schema_hash()
                    )
                ));
            }
        }
        headers.push(header);
    }

    let heads: Vec<PageId> = headers
        .iter()
        .filter(|header| header.prev_page() == NONE_PAGE_ID)
        .map(|header| header.page_id())
        .collect();
    let tails: Vec<PageId> = headers
        .iter()
        .filter(|header| header.next_page() == NONE_PAGE_ID)
        .map(|header| header.page_id())
        .collect();
    if heads.len() != 1 || tails.len() != 1 {
        return Err(m_error!(
            EC::DecodeErr,
            format!(
                "time series file requires exactly one head and one tail, got heads={}, tails={}",
                heads.len(),
                tails.len()
            )
        ));
    }

    let head = heads[0];
    let tail = tails[0];
    let mut current = head;
    let mut visited = vec![false; page_count as usize];
    let mut prev_non_empty_min = None;
    loop {
        if visited[current as usize] {
            return Err(m_error!(
                EC::DecodeErr,
                "time series page chain has a cycle"
            ));
        }
        visited[current as usize] = true;
        let header = &headers[current as usize];
        if let Some(next) = (header.next_page() != NONE_PAGE_ID).then_some(header.next_page()) {
            let next_header = &headers[next as usize];
            if next_header.prev_page() != current {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!("broken page link {} -> {}", current, next)
                ));
            }
        }

        let buf = read_file_exact(file, PAGE_SIZE, page_offset(current)?).await?;
        let page = PageBlockRef::new(&buf);
        if let Some((min_ts, _max_ts)) = page.timestamp_bounds()? {
            if let Some(prev_min) = prev_non_empty_min {
                let page_max = page.timestamp_bounds()?.unwrap().1;
                if page_max > prev_min {
                    return Err(m_error!(
                        EC::DecodeErr,
                        format!(
                            "time series chain order broken between pages: page {} max_ts {} > previous min_ts {}",
                            current, page_max, prev_min
                        )
                    ));
                }
            }
            prev_non_empty_min = Some(min_ts);
        }

        match (header.next_page() != NONE_PAGE_ID).then_some(header.next_page()) {
            Some(next) => current = next,
            None => break,
        }
    }

    if visited.iter().any(|seen| !seen) {
        return Err(m_error!(
            EC::DecodeErr,
            "time series file contains disconnected pages"
        ));
    }

    Ok((Some(head), Some(tail)))
}

fn load_chain_metadata_sync(
    file: &IoFile,
    page_count: PageId,
    expected_schema_hash: u64,
) -> RS<(Option<PageId>, Option<PageId>)> {
    if page_count == 0 {
        return Ok((None, None));
    }

    let mut headers = Vec::with_capacity(page_count as usize);
    for page_id in 0..page_count {
        let buf = read_file_exact_sync(file, PAGE_SIZE, page_offset(page_id)?)?;
        let page = PageBlockRef::new(&buf);
        page.validate_layout()?;
        let header = page.header()?;
        if expected_schema_hash != 0 {
            if header.tuple_format_version() == 0 {
                return Err(m_error!(
                    EC::DecodeErr,
                    "missing tuple format version in page header"
                ));
            }
            if header.tuple_schema_hash() != expected_schema_hash {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!(
                        "page tuple schema hash mismatch: page_id={} expected={} got={}",
                        page_id,
                        expected_schema_hash,
                        header.tuple_schema_hash()
                    )
                ));
            }
        }
        headers.push(header);
    }

    let heads: Vec<PageId> = headers
        .iter()
        .filter(|header| header.prev_page() == NONE_PAGE_ID)
        .map(|header| header.page_id())
        .collect();
    let tails: Vec<PageId> = headers
        .iter()
        .filter(|header| header.next_page() == NONE_PAGE_ID)
        .map(|header| header.page_id())
        .collect();
    if heads.len() != 1 || tails.len() != 1 {
        return Err(m_error!(
            EC::DecodeErr,
            format!(
                "time series file requires exactly one head and one tail, got heads={}, tails={}",
                heads.len(),
                tails.len()
            )
        ));
    }

    let head = heads[0];
    let tail = tails[0];
    let mut current = head;
    let mut visited = vec![false; page_count as usize];
    let mut prev_non_empty_min = None;
    loop {
        if visited[current as usize] {
            return Err(m_error!(
                EC::DecodeErr,
                "time series page chain has a cycle"
            ));
        }
        visited[current as usize] = true;
        let header = &headers[current as usize];
        if let Some(next) = (header.next_page() != NONE_PAGE_ID).then_some(header.next_page()) {
            let next_header = &headers[next as usize];
            if next_header.prev_page() != current {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!("broken page link {} -> {}", current, next)
                ));
            }
        }

        let buf = read_file_exact_sync(file, PAGE_SIZE, page_offset(current)?)?;
        let page = PageBlockRef::new(&buf);
        if let Some((min_ts, _max_ts)) = page.timestamp_bounds()? {
            if let Some(prev_min) = prev_non_empty_min {
                let page_max = page.timestamp_bounds()?.unwrap().1;
                if page_max > prev_min {
                    return Err(m_error!(
                        EC::DecodeErr,
                        format!(
                            "time series chain order broken between pages: page {} max_ts {} > previous min_ts {}",
                            current, page_max, prev_min
                        )
                    ));
                }
            }
            prev_non_empty_min = Some(min_ts);
        }

        match (header.next_page() != NONE_PAGE_ID).then_some(header.next_page()) {
            Some(next) => current = next,
            None => break,
        }
    }

    if visited.iter().any(|seen| !seen) {
        return Err(m_error!(
            EC::DecodeErr,
            "time series file contains disconnected pages"
        ));
    }

    Ok((Some(head), Some(tail)))
}

fn page_offset(page_id: PageId) -> RS<u64> {
    (page_id as u64)
        .checked_mul(PAGE_SIZE as u64)
        .ok_or_else(|| m_error!(EC::IndexOutOfRange, "time series page offset overflow"))
}

async fn open_rw(path: &Path, flags: i32) -> RS<IoFile> {
    file::open(path, flags, FILE_MODE_644).await
}

fn open_rw_sync(path: &Path, flags: i32) -> RS<IoFile> {
    file::open_sync(path, flags, FILE_MODE_644)
}

async fn read_file_exact(file: &IoFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    file::read(file, len, offset).await
}

fn read_file_exact_sync(file: &IoFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    file::read_sync(file, len, offset)
}

fn write_file_all_sync(file: &IoFile, payload: &[u8], offset: u64) -> RS<()> {
    file::write_sync(file, payload, offset)
}

async fn flush_file(file: &IoFile) -> RS<()> {
    file::flush(file).await
}

async fn close_file(file: IoFile) -> RS<()> {
    file::close(file).await
}

fn new_relation_wal_backend(
    base_path: &Path,
    identity: &TimeSeriesFileIdentity,
) -> RS<ChunkedWorkerLogBackend> {
    // Each relation file gets its own physical-log stream so recovery can
    // replay one file independently of the rest of the worker state.
    let log_dir = base_path.join("relation_wal");
    let layout = WorkerLogLayout::new(
        log_dir,
        time_series_log_oid(identity),
        RELATION_WAL_CHUNK_SIZE,
    )?;
    ChunkedWorkerLogBackend::new_direct(layout)
}

fn time_series_log_oid(identity: &TimeSeriesFileIdentity) -> OID {
    identity.partition_id.rotate_left(17)
        ^ identity.table_id.rotate_left(53)
        ^ (identity.file_index as u128).rotate_left(97)
        ^ 0x706c5f74735f66696c655f77616c_u128
}

fn recover_relation_file(
    base_path: &Path,
    identity: &TimeSeriesFileIdentity,
    backend: &ChunkedWorkerLogBackend,
) -> RS<()> {
    // Recovery is file-local: replay the PL stream for this exact file id
    // before opening the data file and rebuilding in-memory metadata.
    let path = TimeSeriesFile::relation_file_path(
        base_path,
        identity.partition_id,
        identity.table_id,
        identity.file_index,
    );
    for chunk_path in backend.chunk_paths_sorted()? {
        let bytes = mudu_sys::fs::read_all(&chunk_path)
            .map_err(|e| m_error!(EC::IOErr, "read time series wal chunk error", e))?;
        if bytes.is_empty() {
            continue;
        }
        let frames = crate::wal::worker_log::decode_frames(&bytes)?;
        let batches = crate::wal::pl_batch::decode_pl_batches(&frames)?;
        for batch in batches {
            for entry in batch.entries {
                if entry.file
                    != (PLFileId {
                        partition_id: identity.partition_id,
                        table_id: identity.table_id,
                        file_index: identity.file_index,
                    })
                {
                    continue;
                }
                apply_recovered_entry(&path, &entry)?;
            }
        }
    }
    Ok(())
}

async fn recover_relation_file_async(
    fs: Arc<dyn AsyncFs>,
    base_path: &Path,
    identity: &TimeSeriesFileIdentity,
    backend: &ChunkedWorkerLogBackend,
) -> RS<()> {
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

fn apply_recovered_entry(path: &Path, entry: &PLEntry) -> RS<()> {
    for op in &entry.ops {
        match op {
            PLOp::Create => ensure_time_series_file_exists_sync(path)?,
            PLOp::Delete => remove_file_if_exists(path)?,
            PLOp::PageUpdate(update) => {
                ensure_time_series_file_exists_sync(path)?;
                let file = open_rw_sync(path, libc::O_CREAT | libc::O_RDWR | file::cloexec_flag())?;
                let result = write_file_all_sync(
                    &file,
                    &update.data,
                    page_offset(update.page_id)? + update.offset as u64,
                );
                let close_result = file::close_sync(file);
                result?;
                close_result?;
            }
        }
    }
    Ok(())
}

async fn apply_recovered_entry_async(fs: &dyn AsyncFs, path: &Path, entry: &PLEntry) -> RS<()> {
    for op in &entry.ops {
        match op {
            PLOp::Create => ensure_time_series_file_exists_async(fs, path).await?,
            PLOp::Delete => fs.remove_file_if_exists(path).await?,
            PLOp::PageUpdate(update) => {
                ensure_time_series_file_exists_async(fs, path).await?;
                let file = fs.open(path, FileOpenOptions::read_write_create()).await?;
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
        self.backend.chunk_paths_sorted()
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
    async fn handle_entry(&self, entry: PLBatch, _start_lsn: crate::wal::lsn::LSN) -> RS<()> {
        for item in &entry.entries {
            if item.file != self.file_id {
                continue;
            }
            apply_recovered_entry_async(self.fs.as_ref(), &self.path, item).await?;
        }
        Ok(())
    }
}

async fn append_file_create_async(
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

fn append_file_create_sync(
    backend: &ChunkedWorkerLogBackend,
    identity: &TimeSeriesFileIdentity,
) -> RS<()> {
    let writer = new_pl_batch_writer(backend.clone());
    writer.append_sync(&PLBatch::new(vec![PLEntry {
        file: PLFileId {
            partition_id: identity.partition_id,
            table_id: identity.table_id,
            file_index: identity.file_index,
        },
        ops: vec![PLOp::Create],
    }]))
}

fn ensure_time_series_file_exists_sync(path: &Path) -> RS<()> {
    if let Some(parent) = path.parent() {
        mudu_sys::fs::create_dir_all(parent)
            .map_err(|e| m_error!(EC::IOErr, "create time series dir error", e))?;
    }
    if path.exists() {
        return Ok(());
    }
    let file = open_rw_sync(path, libc::O_CREAT | libc::O_RDWR | file::cloexec_flag())?;
    file::close_sync(file)
}

async fn ensure_time_series_file_exists_async(fs: &dyn AsyncFs, path: &Path) -> RS<()> {
    if let Some(parent) = path.parent() {
        trace!(path = %path.display(), parent = %parent.display(), "time_series ensure file create_dir_all");
        fs.create_dir_all(parent).await?;
    }
    trace!(path = %path.display(), "time_series ensure file open create");
    let _ = fs.open(path, FileOpenOptions::read_write_create()).await?;
    trace!(path = %path.display(), "time_series ensure file open create done");
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> RS<()> {
    mudu_sys::fs::remove_file_if_exists(path)
}

#[cfg(test)]
mod tests {
    use super::{TimeSeriesFile, TimeSeriesFileIdentity, PAGE_SIZE};
    use crate::async_rt::tokio::fs::TokioFs;
    use crate::storage::page::PageId;
    use project_root::get_project_root;
    use std::sync::Arc;

    fn temp_ts_path(name: &str) -> std::path::PathBuf {
        let root = get_project_root().unwrap();
        root.join("target").join("tmp").join(format!(
            "tsf-{}-{}.dat",
            name,
            mudu_sys::random::uuid_v4()
        ))
    }

    fn temp_relation_base(name: &str) -> std::path::PathBuf {
        let root = get_project_root().unwrap();
        root.join("target").join("tmp").join(format!(
            "tsf-rel-{}-{}",
            name,
            mudu_sys::random::uuid_v4()
        ))
    }

    fn payload(byte: u8, len: usize) -> Vec<u8> {
        vec![byte; len]
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_create_empty_file() {
        let path = temp_ts_path("empty");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();
        assert_eq!(file.page_count(), 0 as PageId);
        assert_eq!(file.head_page_id(), None);
        assert_eq!(file.tail_page_id(), None);
        file.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn insert_get_update_delete_roundtrip() {
        let path = temp_ts_path("roundtrip");
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        file.insert(100, 1, b"v1").await.unwrap();
        file.insert(90, 2, b"v2").await.unwrap();
        file.insert(100, 1, b"v1-new").await.unwrap();

        let row = file.get(100, 1).await.unwrap().unwrap();
        assert_eq!(row.payload, b"v1-new");
        assert_eq!(row.timestamp, 100);
        assert_eq!(row.tuple_id, 1);

        let row = file.get(90, 2).await.unwrap().unwrap();
        assert_eq!(row.payload, b"v2");

        assert!(file.delete(90, 2).await.unwrap());
        assert_eq!(file.get(90, 2).await.unwrap(), None);
        assert!(!file.delete(90, 2).await.unwrap());

        file.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn scan_range_returns_sorted_records() {
        let path = temp_ts_path("scan");
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        file.insert(120, 4, b"d").await.unwrap();
        file.insert(100, 2, b"b").await.unwrap();
        file.insert(100, 1, b"a").await.unwrap();
        file.insert(110, 3, b"c").await.unwrap();
        file.insert(90, 5, b"e").await.unwrap();

        let rows = file.scan_range(95, 115).await.unwrap();
        let keys: Vec<(u64, u64, Vec<u8>)> = rows
            .into_iter()
            .map(|row| (row.timestamp, row.tuple_id, row.payload))
            .collect();
        assert_eq!(
            keys,
            vec![
                (100, 1, b"a".to_vec()),
                (100, 2, b"b".to_vec()),
                (110, 3, b"c".to_vec()),
            ]
        );

        file.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reopen_preserves_records() {
        let path = temp_ts_path("reopen");
        {
            let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();
            file.insert(100, 1, b"alpha").await.unwrap();
            file.insert(80, 2, b"beta").await.unwrap();
            file.flush().await.unwrap();
            file.close().await.unwrap();
        }

        let file = TimeSeriesFile::open_ts_file(&path, false).await.unwrap();
        let row = file.get(100, 1).await.unwrap().unwrap();
        assert_eq!(row.payload, b"alpha");
        let row = file.get(80, 2).await.unwrap().unwrap();
        assert_eq!(row.payload, b"beta");
        assert_eq!(
            file.scan_range(0, 200)
                .await
                .unwrap()
                .into_iter()
                .map(|row| (row.timestamp, row.tuple_id))
                .collect::<Vec<_>>(),
            vec![(80, 2), (100, 1)]
        );
        file.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn insert_creates_multiple_pages_when_page_is_full() {
        let path = temp_ts_path("split");
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        for idx in 0..16u64 {
            let ts = 10_000 - idx;
            let data = payload((idx % 251) as u8, 700);
            file.insert(ts, idx, &data).await.unwrap();
        }

        assert!(file.page_count() > 1);
        assert!(file.head_page_id().is_some());
        assert!(file.tail_page_id().is_some());

        for idx in 0..16u64 {
            let ts = 10_000 - idx;
            let row = file.get(ts, idx).await.unwrap().unwrap();
            assert_eq!(row.timestamp, ts);
            assert_eq!(row.tuple_id, idx);
            assert_eq!(row.payload.len(), 700);
        }

        let rows = file.scan_range(9_980, 10_000).await.unwrap();
        assert_eq!(rows.len(), 16);
        assert!(rows.iter().all(|row| row.payload.len() == 700));

        file.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cached_pages_are_reused_after_writes() {
        let path = temp_ts_path("cache");
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        file.insert(100, 1, b"cached").await.unwrap();
        let page_count = file.page_count();
        assert_eq!(page_count, 1);

        let first = file.get(100, 1).await.unwrap().unwrap();
        let second = file.get(100, 1).await.unwrap().unwrap();
        assert_eq!(first.payload, second.payload);
        assert_eq!(first.page_id, 0);

        let file_len = std::fs::metadata(file.path()).unwrap().len() as usize;
        assert_eq!(file_len % PAGE_SIZE, 0);

        file.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn integrated_api_flow_covers_all_public_operations() {
        let path = temp_ts_path("integrated");
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        assert_eq!(file.page_count(), 0);
        assert_eq!(file.head_page_id(), None);
        assert_eq!(file.tail_page_id(), None);
        assert_eq!(file.get(1, 1).await.unwrap(), None);
        assert!(file.scan_range(1, 10).await.unwrap().is_empty());
        assert!(!file.delete(1, 1).await.unwrap());

        for idx in 0..12u64 {
            let ts = 1_000 - idx;
            let value = payload((idx % 251) as u8, 768);
            file.insert(ts, idx, &value).await.unwrap();
        }

        assert!(file.page_count() > 1);
        let head = file.head_page_id().unwrap();
        let tail = file.tail_page_id().unwrap();
        assert!(head <= tail);

        for idx in 0..12u64 {
            let ts = 1_000 - idx;
            let row = file.get(ts, idx).await.unwrap().unwrap();
            assert_eq!(row.timestamp, ts);
            assert_eq!(row.tuple_id, idx);
            assert_eq!(row.payload, payload((idx % 251) as u8, 768));
        }

        let rows = file.scan_range(993, 1_000).await.unwrap();
        let keys: Vec<(u64, u64)> = rows
            .iter()
            .map(|row| (row.timestamp, row.tuple_id))
            .collect();
        assert_eq!(
            keys,
            vec![
                (993, 7),
                (994, 6),
                (995, 5),
                (996, 4),
                (997, 3),
                (998, 2),
                (999, 1),
                (1000, 0),
            ]
        );

        file.insert(997, 3, b"updated").await.unwrap();
        let updated = file.get(997, 3).await.unwrap().unwrap();
        assert_eq!(updated.payload, b"updated");

        assert!(file.delete(995, 5).await.unwrap());
        assert_eq!(file.get(995, 5).await.unwrap(), None);
        assert!(!file.delete(995, 5).await.unwrap());

        file.flush().await.unwrap();
        let persisted_page_count = file.page_count();
        let persisted_head = file.head_page_id();
        let persisted_tail = file.tail_page_id();
        file.close().await.unwrap();

        let reopened = TimeSeriesFile::open_ts_file(&path, false).await.unwrap();
        assert_eq!(reopened.page_count(), persisted_page_count);
        assert_eq!(reopened.head_page_id(), persisted_head);
        assert_eq!(reopened.tail_page_id(), persisted_tail);
        assert_eq!(reopened.get(995, 5).await.unwrap(), None);
        assert_eq!(
            reopened.get(997, 3).await.unwrap().unwrap().payload,
            b"updated"
        );

        let reopened_rows = reopened.scan_range(989, 1_000).await.unwrap();
        let reopened_keys: Vec<(u64, u64)> = reopened_rows
            .iter()
            .map(|row| (row.timestamp, row.tuple_id))
            .collect();
        assert_eq!(
            reopened_keys,
            vec![
                (989, 11),
                (990, 10),
                (991, 9),
                (992, 8),
                (993, 7),
                (994, 6),
                (996, 4),
                (997, 3),
                (998, 2),
                (999, 1),
                (1000, 0),
            ]
        );
        reopened.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn wal_recovers_relation_file_after_data_loss() {
        let base = temp_relation_base("recover");
        let identity = TimeSeriesFileIdentity {
            partition_id: 7,
            table_id: 11,
            file_index: 0,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );

        let mut file =
            TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0xfeed_beef, true)
                .unwrap();
        file.insert_sync(100, 1, b"alpha").unwrap();
        file.insert_sync(90, 2, b"beta").unwrap();
        file.delete_sync(90, 2).unwrap();
        file.close_sync().unwrap();
        std::fs::remove_file(&path).unwrap();

        let reopened =
            TimeSeriesFile::open_relation_file_sync(&base, identity, 0xfeed_beef, false).unwrap();
        assert_eq!(
            reopened.get_sync(100, 1).unwrap().unwrap().payload,
            b"alpha".to_vec()
        );
        assert_eq!(reopened.get_sync(90, 2).unwrap(), None);
        reopened.close_sync().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn wal_recovers_empty_file_from_create_record() {
        let base = temp_relation_base("create");
        let identity = TimeSeriesFileIdentity {
            partition_id: 17,
            table_id: 23,
            file_index: 1,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );

        let file =
            TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0x1, true).unwrap();
        file.close_sync().unwrap();
        std::fs::remove_file(&path).unwrap();

        let reopened =
            TimeSeriesFile::open_relation_file_sync(&base, identity, 0x1, false).unwrap();
        assert_eq!(reopened.page_count(), 0);
        reopened.close_sync().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wal_recovers_relation_file_after_data_loss_async() {
        let base = temp_relation_base("recover-async");
        let identity = TimeSeriesFileIdentity {
            partition_id: 27,
            table_id: 31,
            file_index: 2,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );

        let mut file =
            TimeSeriesFile::open_relation_file(&base, identity.clone(), 0x1234_5678, true)
                .await
                .unwrap();
        file.insert(100, 1, b"alpha").await.unwrap();
        file.insert(90, 2, b"beta").await.unwrap();
        file.delete(90, 2).await.unwrap();
        file.close().await.unwrap();
        std::fs::remove_file(&path).unwrap();

        let reopened = TimeSeriesFile::open_relation_file(&base, identity, 0x1234_5678, false)
            .await
            .unwrap();
        assert_eq!(
            reopened.get_sync(100, 1).unwrap().unwrap().payload,
            b"alpha".to_vec()
        );
        assert_eq!(reopened.get_sync(90, 2).unwrap(), None);
        reopened.close_sync().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wal_recovers_relation_file_with_injected_async_fs() {
        let base = temp_relation_base("recover-async-fs");
        let identity = TimeSeriesFileIdentity {
            partition_id: 37,
            table_id: 41,
            file_index: 3,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );
        let fs = Arc::new(TokioFs::new());

        let mut file = TimeSeriesFile::open_relation_file_with_fs(
            fs.clone(),
            &base,
            identity.clone(),
            0x55aa_aa55,
            true,
        )
        .await
        .unwrap();
        file.insert(100, 1, b"alpha").await.unwrap();
        file.insert(90, 2, b"beta").await.unwrap();
        file.close().await.unwrap();
        std::fs::remove_file(&path).unwrap();

        let reopened =
            TimeSeriesFile::open_relation_file_with_fs(fs, &base, identity, 0x55aa_aa55, false)
                .await
                .unwrap();
        assert_eq!(
            reopened.get_sync(100, 1).unwrap().unwrap().payload,
            b"alpha".to_vec()
        );
        assert_eq!(
            reopened.get_sync(90, 2).unwrap().unwrap().payload,
            b"beta".to_vec()
        );
        reopened.close_sync().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn wal_replays_terminal_delete_before_open() {
        let base = temp_relation_base("delete");
        let identity = TimeSeriesFileIdentity {
            partition_id: 29,
            table_id: 31,
            file_index: 0,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );

        let mut file =
            TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0x2, true).unwrap();
        file.insert_sync(42, 9, b"payload").unwrap();
        file.delete_file_sync().unwrap();

        let stray = TimeSeriesFile::open_ts_file_sync(&path, true).unwrap();
        stray.close_sync().unwrap();
        assert!(path.exists());

        let err = TimeSeriesFile::open_relation_file_sync(&base, identity, 0x2, false)
            .err()
            .unwrap();
        assert!(!path.exists());
        assert!(err.to_string().contains("open file error"));
        std::fs::remove_dir_all(base).unwrap();
    }
}
