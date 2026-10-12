use super::io::{ensure_time_series_file_exists_async, page_offset};
use super::page::{empty_page_image, finalize_page_image_checksum, stamp_page_image_lsn};
use super::{TimeSeriesFile, TimeSeriesFileIdentity};
use crate::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::page::page_block_ref_mut::PageBlockRefMut;
use ::wal::lsn::LSN;
use ::wal::pl_batch::{
    new_pl_batch_worker_log, new_pl_batch_writer, NoopPLBatchRecoveryHandler, PLBatch,
};
use ::wal::pl_entry::{PLEntry, PLFileId, PLOp, PageDelta};
use ::wal::typed_worker_log::AsyncWorkerLogRecoveryHandler;
use ::wal::worker_log::AsyncWorkerLogRecoverySource;
use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogLayout};
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_file::AsyncFile;
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
    log.recover_async_with_handler(&mut source, &*handler).await
}

async fn apply_recovered_entry_async(
    fs: &dyn AsyncFs,
    path: &Path,
    entry: &PLEntry,
    batch_lsn: LSN,
) -> RS<()> {
    // The data file is opened lazily on the first page delta and reused for
    // the rest of the entry; a Delete drops the handle before unlinking.
    let mut file: Option<Arc<dyn AsyncFile>> = None;
    for op in &entry.ops {
        match op {
            PLOp::Create => ensure_time_series_file_exists_async(fs, path).await?,
            PLOp::Delete => {
                file = None;
                fs.remove_file_if_exists(path).await?;
            }
            PLOp::PageDelta(delta) => {
                if file.is_none() {
                    ensure_time_series_file_exists_async(fs, path).await?;
                    file = Some(fs.open(path, FileOptions::read_write_create()).await?);
                }
                if let Some(file) = file.as_ref() {
                    apply_recovered_page_delta(file.as_ref(), delta, batch_lsn).await?;
                }
            }
        }
    }
    Ok(())
}

/// Replays one record-level page delta with read-modify-write against the
/// data file.
///
/// Every page image on disk carries the WAL LSN of the batch that produced
/// it (stamped at persist time, see `stamp_page_image_lsn`). A delta is
/// applied only when the page's stamped LSN is older than the delta's batch
/// LSN; otherwise the page already reflects the batch (flushed before the
/// crash, or a repeated replay) and re-applying the record-level delta
/// would resurrect records that later batches moved or deleted.
///
/// When applied, the removes / upserts / link updates run with the same
/// page primitives the write path uses, the page is stamped with the batch
/// LSN, the tailer checksum is finalized, and the page is written back in
/// place.
async fn apply_recovered_page_delta(
    file: &dyn AsyncFile,
    delta: &PageDelta,
    batch_lsn: LSN,
) -> RS<()> {
    let offset = page_offset(delta.page_id)?;
    let disk_image = read_recoverable_page_image(file, offset).await?;
    if let Some(disk_image) = disk_image.as_ref() {
        if PageBlockRef::new(disk_image).header()?.lsn() >= batch_lsn {
            return Ok(());
        }
    }

    let mut image = recovered_base_image(delta, disk_image)?;
    apply_recovered_upserts(&mut image, delta)?;

    // Stamp the batch's WAL LSN so a later replay recognizes this page as
    // already reflecting the batch, then finalize the checksum before the
    // image touches the data file.
    stamp_page_image_lsn(&mut image, batch_lsn)?;
    finalize_page_image_checksum(&mut image)?;
    file.write_all_at(offset, &image).await?;
    Ok(())
}

/// Reads the current on-disk page image at `offset`, or returns `None` when
/// the data file does not reach that page yet.
///
/// On-disk pages are always checksum-finalized; a validation failure here
/// means a torn write or corruption, which must fail recovery loudly instead
/// of producing a silently divergent page.
async fn read_recoverable_page_image(file: &dyn AsyncFile, offset: u64) -> RS<Option<Vec<u8>>> {
    let page_present = file.file_len().await? >= offset + PAGE_SIZE as u64;
    if !page_present {
        return Ok(None);
    }
    let image = file.read_exact_at(offset, PAGE_SIZE).await?;
    PageBlockRef::new(&image).validate_layout()?;
    Ok(Some(image))
}

/// Builds the page image the delta's upserts are applied onto: either the
/// fresh page the delta's `init` fully defines, or the validated on-disk
/// image with the delta's link updates and removes applied.
fn recovered_base_image(delta: &PageDelta, disk_image: Option<Vec<u8>>) -> RS<Vec<u8>> {
    if let Some(init) = delta.init.as_ref() {
        // The delta fully defines a page allocated by this batch, so it is
        // rebuilt from scratch: the page is absent, or the on-disk image
        // predates the batch (e.g. a mid-flush crash left an older image).
        let mut image = empty_page_image(
            delta.page_id,
            init.tuple_format_version,
            init.tuple_schema_hash,
            init.tuple_flags,
        )?;
        PageBlockRefMut::new(&mut image).set_page_links(init.prev_page, init.next_page)?;
        return Ok(image);
    }
    let Some(mut image) = disk_image else {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "wal replay delta for page {} without init, but the data file has no such page",
                delta.page_id
            )
        ));
    };
    if let Some(links) = delta.links.as_ref() {
        PageBlockRefMut::new(&mut image).set_page_links(links.prev_page, links.next_page)?;
    }
    for key in &delta.removes {
        let slot_index = PageBlockRef::new(&image)
            .find_slot_index(key.timestamp, key.tuple_id)?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::Decode,
                    format!(
                        "wal replay remove of ({}, {}) on page {} found no such record",
                        key.timestamp, key.tuple_id, delta.page_id
                    )
                )
            })?;
        PageBlockRefMut::new(&mut image).delete_record(slot_index)?;
    }
    Ok(image)
}

/// Applies the delta's upserts in order with the same page primitives the
/// write path uses.
fn apply_recovered_upserts(image: &mut [u8], delta: &PageDelta) -> RS<()> {
    for record in &delta.upserts {
        let slot_index =
            PageBlockRef::new(image).find_slot_index(record.timestamp, record.tuple_id)?;
        match slot_index {
            Some(slot_index) => {
                PageBlockRefMut::new(image).update_record(
                    slot_index,
                    record.timestamp,
                    record.tuple_id,
                    &record.payload,
                )?;
            }
            None => {
                let insert = PageBlockRefMut::new(image).insert_record(
                    record.timestamp,
                    record.tuple_id,
                    &record.payload,
                );
                match insert {
                    Ok(_) => {}
                    Err(err) if err.ec() == ErrorCode::InsufficientBufferSpace => {
                        // The replayed insert order can pack a page slightly
                        // worse than the order the writer used (record
                        // alignment padding is order-dependent); compacting
                        // reclaims the fragmentation before one retry.
                        PageBlockRefMut::new(image).compact()?;
                        PageBlockRefMut::new(image).insert_record(
                            record.timestamp,
                            record.tuple_id,
                            &record.payload,
                        )?;
                    }
                    Err(err) => return Err(err),
                }
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
impl AsyncWorkerLogRecoveryHandler<PLBatch> for RelationWalRecoveryHandler {
    async fn handle_entry(&self, entry: PLBatch, start_lsn: LSN) -> RS<()> {
        scoped_task_trace!();
        for item in &entry.entries {
            if item.file != self.file_id {
                continue;
            }
            apply_recovered_entry_async(self.fs.as_ref(), &self.path, item, start_lsn).await?;
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
