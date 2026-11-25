//! Per-worker business snapshots: the local snapshot routine shared by
//! both replication modes, and the on-disk layout the snapshot transfer
//! (offer/fetch) serves and restores.
//!
//! A snapshot self-seals at a committed point `(committed_lsn, seg_hash)`:
//! the routine flushes dirty data pages, fsyncs the worker's relation
//! files, copies them together with a dump of the in-memory kv store into a
//! versioned snapshot directory, and publishes a manifest LAST (a directory
//! is only valid once its manifest decodes and every listed file is
//! present with matching checksums). Files and directories are synced before
//! atomic manifest publication; a version without a manifest is incomplete.
//!
//! Layout (`<data_dir>/snapshot/worker_<N>/`):
//!
//! - `v<committed_lsn>/relation/*.dat` — flushed relation files of the
//!   worker's partitions, including partition 0 on worker 0;
//! - `v<committed_lsn>/meta/relation/*.dat` — shared catalogs on worker 0;
//! - `v<committed_lsn>/kv.mp` — msgpack dump of the in-memory kv store
//!   (latest version per key, tombstones included);
//! - `v<committed_lsn>/recovery.mp` — local clocks and participant image at the row cut;
//! - `v<committed_lsn>/manifest.mp` — the [`mudu_contract::protocol::ReplSnapshotManifest`],
//!   fetched last so an incomplete directory is never offered.
//!
//! Older versions are reclaimed once a newer manifest is durable; the
//! loader always picks the highest valid version, so a leftover partial
//! version is harmless.

mod base;
mod install;
mod startup;
pub use base::recovery_base_resume;
pub use startup::recover_worker_storage;
mod manifest;
mod recovery;
pub(crate) use install::InstallJournal;
pub use install::{ensure_no_pending_installs, recover_pending_installs};
pub use manifest::{seal_snapshot, validate_snapshot_manifest};
pub use recovery::restore_snapshot_recovery_metadata;

use crate::seg_hash::SegHash;
use ::worker::worker::flush_worker_dirty_pages;
use ::worker::worker::WorkerRuntime;
use ::xcontract::worker_storage::KvDumpEntry;
use mudu::common::crc::crc64;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{ReplSnapshotFileEntry, ReplSnapshotManifest};
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::file_options::FileOptions;
#[cfg(test)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{trace, warn};

/// Directory name under the data dir holding every worker's snapshots.
pub(crate) const SNAPSHOT_DIR_NAME: &str = "snapshot";
/// Manifest file name inside a snapshot version directory.
pub(crate) const MANIFEST_FILE_NAME: &str = "manifest.mp";
/// kv dump file name inside a snapshot version directory.
pub(crate) const KV_FILE_NAME: &str = "kv.mp";
/// Relation directory name (inside a snapshot version and in the data dir).
pub(crate) const RELATION_DIR_NAME: &str = "relation";

/// Root of this worker's snapshot versions: `<data_dir>/snapshot/worker_<N>`.
pub(crate) fn snapshot_worker_dir(data_dir: &str, worker_index: u32) -> PathBuf {
    PathBuf::from(data_dir)
        .join(SNAPSHOT_DIR_NAME)
        .join(format!("worker_{worker_index}"))
}

/// The snapshot version directory for one committed LSN.
pub fn snapshot_version_dir(data_dir: &str, worker_index: u32, committed_lsn: u64) -> PathBuf {
    snapshot_worker_dir(data_dir, worker_index).join(format!("v{committed_lsn}"))
}

/// Loads the newest sealed snapshot after identity, length and CRC checks.
/// Unsealed versions are ignored. A corrupt sealed version fails closed:
/// its prefix may already have been reclaimed, so an older cut is not a safe
/// fallback. Returns `None` only when no sealed snapshot exists.
pub(crate) fn load_latest_manifest(
    data_dir: &str,
    worker_index: u32,
) -> RS<Option<ReplSnapshotManifest>> {
    let worker_dir = snapshot_worker_dir(data_dir, worker_index);
    if !mudu_sys::fs::sync::sync_path_exists(&worker_dir) {
        return Ok(None);
    }
    let mut versions: Vec<(u64, PathBuf)> = Vec::new();
    for path in mudu_sys::fs::sync::read_dir(&worker_dir)? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(lsn_text) = name.strip_prefix('v') else {
            continue;
        };
        if let Ok(lsn) = lsn_text.parse::<u64>() {
            versions.push((lsn, path));
        }
    }
    versions.sort_by_key(|(lsn, _)| *lsn);
    for (lsn, dir) in versions.into_iter().rev() {
        if let Some(manifest) = manifest::read_sealed(&dir)? {
            if manifest.worker_index != worker_index || manifest.committed_lsn != lsn {
                return Err(mudu_error!(
                    ErrorCode::CorruptedData,
                    "snapshot directory identity mismatch"
                ));
            }
            return Ok(Some(manifest));
        }
    }
    Ok(None)
}

/// Verifies every file of the snapshot in `dir` against the manifest
/// (length and CRC64). Used by the restoring replica after the fetch.
pub fn verify_snapshot_files(dir: &Path, manifest: &ReplSnapshotManifest) -> RS<()> {
    validate_snapshot_manifest(manifest)?;
    for entry in &manifest.files {
        let path = dir.join(&entry.path);
        let bytes = mudu_sys::fs::sync::read(&path)?;
        if bytes.len() as u64 != entry.len {
            return Err(mudu_error!(
                ErrorCode::CorruptedData,
                format!(
                    "snapshot file {} length mismatch: manifest {}, actual {}",
                    entry.path,
                    entry.len,
                    bytes.len()
                )
            ));
        }
        let crc = crc64(&bytes);
        if crc != entry.crc64 {
            return Err(mudu_error!(
                ErrorCode::CorruptedData,
                format!(
                    "snapshot file {} CRC64 mismatch: manifest {:x}, actual {:x}",
                    entry.path, entry.crc64, crc
                )
            ));
        }
    }
    recovery::read(dir, manifest)?;
    Ok(())
}

/// Reads a page of a snapshot file (source side of `ReplSnapshotFetch`).
/// `path` must stay inside `dir` (no traversal).
pub(crate) async fn read_snapshot_page(
    dir: &Path,
    path: &str,
    offset: u64,
    len: u64,
) -> RS<Vec<u8>> {
    manifest::validate_path(path)?;
    let rel = Path::new(path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("snapshot fetch path {path} escapes the snapshot directory")
        ));
    }
    let full = dir.join(rel);
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    if !fs.path_exists(&full).await? {
        return Err(mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("snapshot file {} not found", full.display())
        ));
    }
    let file_len = fs.metadata_len(&full).await?;
    if offset >= file_len {
        return Ok(Vec::new());
    }
    let read_len = len.min(file_len - offset);
    let file = fs.open(&full, FileOptions::read_only()).await?;
    file.read_exact_at(offset, read_len as usize).await
}

/// Takes a snapshot of one worker at the committed point. Called from the
/// worker's serial replication point (the raft promote applier, the stream
/// sink, or the stream primary's commit watermark): all content the
/// manifest claims to cover is durable by construction.
///
/// The relation copy is the whole-file variant; snapshot files are read
/// into memory (bounded by the relation file sizes).
pub async fn take_worker_snapshot(
    worker: &WorkerRuntime,
    worker_index: u32,
    committed_lsn: u64,
    seg_hash: SegHash,
    raft_term: u64,
    raft_index: u64,
) -> RS<ReplSnapshotManifest> {
    // Hold this through manifest publication. Checking history before/after
    // copying alone leaves a window for a new prepare to enter after the
    // final check but before the snapshot is advertised.
    if worker.worker_index() != worker_index as usize {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "snapshot worker identity mismatch"
        ));
    }
    let participant_storage = worker.storage();
    let _participant_cut = participant_storage.capture_participant_snapshot().await?;
    let resume = committed_lsn
        .checked_add(1)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "snapshot WAL LSN exhausted"))?;
    if raft_index != 0 || worker.raft_share().is_some() {
        _participant_cut.require_wal_resume(resume)?;
    }
    if worker.storage().has_participant_history()? {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "participant WAL history requires a participant-aware snapshot"
        ));
    }
    let data_dir = worker.storage().relation_path().to_string();
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    // 1. flush + fsync: the copied data files then cover every commit up to
    // the snapshot point durably (the fsync is new persistence work on this
    // path; the snapshot threshold keeps its frequency bounded).
    flush_worker_dirty_pages(worker).await?;
    fsync_relation_files(&data_dir, fs.as_ref()).await?;

    let version_dir = snapshot_version_dir(&data_dir, worker_index, committed_lsn);
    if let Some(existing) =
        manifest::reusable_snapshot(&version_dir, worker_index, committed_lsn, seg_hash)?
    {
        base::publish(&data_dir, worker.worker_id(), &existing).await?;
        return Ok(existing);
    }
    if mudu_sys::fs::sync::sync_path_exists(&version_dir) {
        mudu_sys::fs::sync::sync_remove_dir_all(&version_dir)?;
    }
    crate::durable_io::ensure_dir(fs.as_ref(), &version_dir.join(RELATION_DIR_NAME)).await?;

    // 2. copy the relation files of this worker's partitions.
    let mut files = copy_relation_files(worker, &fs, &data_dir, &version_dir).await?;
    if worker_index == 0 {
        files.extend(copy_catalog_files(&fs, &data_dir, &version_dir).await?);
    }

    // 3. dump the in-memory kv store (latest version per key).
    files.push(dump_kv_into(worker, &fs, &version_dir).await?);
    files.push(recovery::write(&fs, &version_dir, worker_index, resume, &_participant_cut).await?);

    // 4. manifest LAST: its presence marks the version as valid.
    let manifest = ReplSnapshotManifest {
        version: 2,
        worker_index,
        committed_lsn,
        seg_hash,
        raft_term,
        raft_index,
        taken_at_ms: mudu_sys::time::system_time_now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        files,
    };
    seal_snapshot(&version_dir, &manifest).await?;
    base::publish(&data_dir, worker.worker_id(), &manifest).await?;

    // 5. reclaim older versions now that this one is valid.
    let worker_dir = snapshot_worker_dir(&data_dir, worker_index);
    reclaim_old_versions(&worker_dir, &version_dir)?;
    trace!(
        worker_index,
        committed_lsn,
        files = manifest.files.len(),
        "worker snapshot taken"
    );
    crate::counters::note_snapshot_taken(worker.server_instance_id());
    Ok(manifest)
}

/// Shared catalogs are stored under `meta/relation`, separately from data
/// relations. Worker 0 includes them so a fresh restore can resolve schemas.
async fn copy_catalog_files(
    fs: &Arc<dyn AsyncFs>,
    data_dir: &str,
    version_dir: &Path,
) -> RS<Vec<ReplSnapshotFileEntry>> {
    let meta_dir = PathBuf::from(data_dir).join("meta");
    fsync_relation_files(&meta_dir.to_string_lossy(), fs.as_ref()).await?;
    let relation_dir = meta_dir.join(RELATION_DIR_NAME);
    let mut files = Vec::new();
    if fs.path_exists(&relation_dir).await? {
        for path in fs.read_dir(&relation_dir).await? {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.ends_with(".dat") {
                let relative = format!("meta/{RELATION_DIR_NAME}/{name}");
                files.push(
                    copy_file_into(fs, &path, &version_dir.join(&relative), &relative).await?,
                );
            }
        }
    }
    Ok(files)
}

/// Copies the relation files of this worker's partitions into the snapshot
/// version directory and returns their manifest entries.
async fn copy_relation_files(
    worker: &WorkerRuntime,
    fs: &Arc<dyn AsyncFs>,
    data_dir: &str,
    version_dir: &Path,
) -> RS<Vec<ReplSnapshotFileEntry>> {
    let mut files = Vec::new();
    let mut partitions: Vec<String> = worker
        .partition_ids()
        .iter()
        .map(|partition| format!("{partition}."))
        .collect();
    // Global catalogs and queues live on partition 0, independently of the
    // random default partition assigned to worker 0 by the worker registry.
    if worker.worker_index() == 0 && !partitions.iter().any(|p| p == "0.") {
        partitions.push("0.".to_string());
    }
    let relation_dir = PathBuf::from(data_dir).join(RELATION_DIR_NAME);
    if fs.path_exists(&relation_dir).await? {
        for path in fs.read_dir(&relation_dir).await? {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.ends_with(".dat") || !partitions.iter().any(|p| name.starts_with(p.as_str())) {
                continue;
            }
            let rel_path = format!("{RELATION_DIR_NAME}/{name}");
            let entry = copy_file_into(fs, &path, &version_dir.join(&rel_path), &rel_path).await?;
            files.push(entry);
        }
    }
    Ok(files)
}

/// Dumps the in-memory kv store (latest version per key) into the snapshot
/// version directory and returns its manifest entry.
async fn dump_kv_into(
    worker: &WorkerRuntime,
    fs: &Arc<dyn AsyncFs>,
    version_dir: &Path,
) -> RS<ReplSnapshotFileEntry> {
    let kv_entries = worker.storage().dump_kv_latest().await;
    let kv_bytes = rmp_serde::to_vec(&kv_entries)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode kv snapshot dump error", e))?;
    write_bytes_into(fs, &version_dir.join(KV_FILE_NAME), KV_FILE_NAME, &kv_bytes).await
}

/// Removes every snapshot version of the worker except `keep`; called once
/// the new version's manifest is durable. Reclaim failures only log.
fn reclaim_old_versions(worker_dir: &Path, keep: &Path) -> RS<()> {
    if install::has_pending_install(worker_dir) {
        return Ok(());
    }
    let pinned = base::pinned_version(worker_dir)?;
    for path in mudu_sys::fs::sync::read_dir(worker_dir)? {
        if pinned.as_ref() != Some(&path)
            && path != keep
            && path
                .file_name()
                .and_then(|p| p.to_str())
                .is_some_and(|name| {
                    name.strip_prefix('v')
                        .is_some_and(|lsn| lsn.parse::<u64>().is_ok())
                })
        {
            if let Err(err) = mudu_sys::fs::sync::sync_remove_dir_all(&path) {
                warn!(path = %path.display(), "snapshot old version reclaim failed: {err}");
            }
        }
    }
    Ok(())
}

/// fsyncs every relation data file (snapshot durability precondition).
async fn fsync_relation_files(data_dir: &str, fs: &dyn AsyncFs) -> RS<()> {
    let relation_dir = PathBuf::from(data_dir).join(RELATION_DIR_NAME);
    if !fs.path_exists(&relation_dir).await? {
        return Ok(());
    }
    for path in fs.read_dir(&relation_dir).await? {
        let is_dat = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".dat"));
        if !is_dat {
            continue;
        }
        let file = fs.open(&path, FileOptions::read_write_create()).await?;
        file.fsync().await?;
    }
    Ok(())
}

/// Copies one file into the snapshot version, fsyncs the copy and records
/// its manifest entry.
async fn copy_file_into(
    fs: &Arc<dyn AsyncFs>,
    src: &Path,
    dest: &Path,
    rel_path: &str,
) -> RS<ReplSnapshotFileEntry> {
    let bytes = fs.read_all(src).await?;
    write_bytes_into(fs, dest, rel_path, &bytes).await
}

/// Writes `bytes` to `dest` (fsynced) and returns the manifest entry.
async fn write_bytes_into(
    fs: &Arc<dyn AsyncFs>,
    dest: &Path,
    rel_path: &str,
    bytes: &[u8],
) -> RS<ReplSnapshotFileEntry> {
    if let Some(parent) = dest.parent() {
        fs.create_dir_all(parent).await?;
    }
    let file = fs.open(dest, FileOptions::read_write_create()).await?;
    file.write_all_at(0, bytes).await?;
    file.fsync().await?;
    Ok(ReplSnapshotFileEntry {
        path: rel_path.to_string(),
        len: bytes.len() as u64,
        crc64: crc64(bytes),
    })
}

/// Writes bytes synchronously with an fsync (manifest publish).
#[cfg(test)]
fn write_file_sync(path: &Path, bytes: &[u8]) -> RS<()> {
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = options.open(path)?;
    file.write_all(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "write snapshot manifest error", e))?;
    file.sync_data()?;
    Ok(())
}

/// Decodes the kv dump of a snapshot version directory.
pub fn read_kv_dump(dir: &Path) -> RS<Vec<KvDumpEntry>> {
    let bytes = mudu_sys::fs::sync::read(dir.join(KV_FILE_NAME))?;
    rmp_serde::from_slice(&bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode kv snapshot dump error", e))
}

/// Installs the relation files of a fetched snapshot into the data dir:
/// each manifest `relation/...` entry replaces the live file (content
/// replace; the storage reload afterwards reopens relations from the new
/// content). The kv dump stays in the snapshot directory; its content is
/// restored into the in-memory store by the caller.
pub async fn install_snapshot_relation_files(
    data_dir: &str,
    snapshot_dir: &Path,
    manifest: &ReplSnapshotManifest,
) -> RS<()> {
    verify_snapshot_files(snapshot_dir, manifest)?;
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    let relation_prefix = format!("{RELATION_DIR_NAME}/");
    let catalog_prefix = format!("meta/{RELATION_DIR_NAME}/");
    for entry in &manifest.files {
        let dest = if let Some(name) = entry.path.strip_prefix(&relation_prefix) {
            PathBuf::from(data_dir).join(RELATION_DIR_NAME).join(name)
        } else if let Some(name) = entry.path.strip_prefix(&catalog_prefix) {
            PathBuf::from(data_dir)
                .join("meta")
                .join(RELATION_DIR_NAME)
                .join(name)
        } else {
            continue;
        };
        let bytes = fs.read_all(&snapshot_dir.join(&entry.path)).await?;
        if let Some(parent) = dest.parent() {
            crate::durable_io::ensure_dir(fs.as_ref(), parent).await?;
        }
        crate::durable_io::publish(fs.as_ref(), &dest, &bytes).await?;
    }
    Ok(())
}

/// Byte cap of one snapshot fetch page.
const SNAPSHOT_FETCH_PAGE: u64 = 4 * 1024 * 1024;

/// Timeout of one snapshot fetch round-trip.
const SNAPSHOT_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Downloads every file of `manifest` from `addr` into `dest_dir`
/// (the local snapshot version directory), verifying every file before
/// publishing the manifest. Only unsealed partial versions may be replaced;
/// an identical sealed version is reused, never deleted by a retry.
pub(crate) async fn fetch_snapshot(
    addr: &str,
    worker_index: u32,
    manifest: &ReplSnapshotManifest,
    dest_dir: &Path,
) -> RS<()> {
    validate_snapshot_manifest(manifest)?;
    if manifest.worker_index != worker_index {
        return Err(mudu_error!(
            ErrorCode::CorruptedData,
            "snapshot worker identity mismatch"
        ));
    }
    if let Some(existing) = manifest::read_sealed(dest_dir)? {
        return if existing == *manifest {
            Ok(())
        } else {
            Err(mudu_error!(
                ErrorCode::InvalidState,
                "refusing to overwrite a sealed snapshot"
            ))
        };
    }
    if mudu_sys::fs::sync::sync_path_exists(dest_dir) {
        mudu_sys::fs::sync::sync_remove_dir_all(dest_dir)?;
    }
    crate::durable_io::ensure_dir(
        mudu_sys::default_sys_io_context()
            .provider_arc()
            .fs_arc()
            .as_ref(),
        dest_dir,
    )
    .await?;
    let mut stream = mudu_sys::net::AsyncTcpStream::connect(addr)
        .await
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("connect snapshot source {addr} error"),
                e
            )
        })?;
    stream
        .set_nodelay(true)
        .map_err(|e| mudu_error!(ErrorCode::Network, "set snapshot fetch nodelay error", e))?;
    for entry in &manifest.files {
        fetch_snapshot_file(&mut stream, worker_index, manifest, entry, dest_dir).await?;
    }
    seal_snapshot(dest_dir, manifest).await
}

/// Pages one snapshot file from the source and writes it fsynced into the
/// local snapshot version directory.
async fn fetch_snapshot_file(
    stream: &mut mudu_sys::net::AsyncTcpStream,
    worker_index: u32,
    manifest: &ReplSnapshotManifest,
    entry: &ReplSnapshotFileEntry,
    dest_dir: &Path,
) -> RS<()> {
    use mudu_contract::protocol::{
        decode_error_response, decode_repl_snapshot_resp_frame, encode_repl_snapshot_fetch_request,
        MessageType, ReplSnapshotFetchReq,
    };
    use mudu_sys::tokio::io::AsyncWriteExt;

    let dest = dest_dir.join(&entry.path);
    if let Some(parent) = dest.parent() {
        mudu_sys::fs::sync::create_dir_all(parent)?;
    }
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    let file = fs.open(&dest, FileOptions::read_write_create()).await?;
    let mut offset = 0u64;
    let mut request_id = 1u64;
    let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; 8192];
    loop {
        let page_len = SNAPSHOT_FETCH_PAGE.min(entry.len - offset);
        let request = encode_repl_snapshot_fetch_request(
            request_id,
            &ReplSnapshotFetchReq::new(
                worker_index,
                manifest.committed_lsn,
                entry.path.clone(),
                offset,
                page_len,
            ),
        )?;
        request_id += 1;
        stream
            .write_all(&request)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "write snapshot fetch error", e))?;
        let data = mudu_sys::timeout(SNAPSHOT_FETCH_TIMEOUT, async {
            let (_stop_tx, mut stop_rx) = mudu_sys::sync::async_::stop_flag::stop_channel();
            loop {
                let Some(frame) =
                    crate::read_next_frame(stream, &mut read_buf, &mut chunk, &mut stop_rx).await?
                else {
                    return Err(mudu_error!(
                        ErrorCode::Network,
                        "snapshot source closed the connection mid-fetch"
                    ));
                };
                match frame.header().message_type() {
                    MessageType::ReplSnapshotResp => {
                        let resp = decode_repl_snapshot_resp_frame(&frame)?;
                        if resp.worker_id() == worker_index
                            && resp.path() == entry.path
                            && resp.offset() == offset
                        {
                            return Ok(resp.into_data());
                        }
                    }
                    MessageType::Error => {
                        let error = decode_error_response(&frame)?;
                        return Err(mudu_error!(
                            ErrorCode::from_u32(error.code()).unwrap_or(ErrorCode::Internal),
                            format!("snapshot source returned error: {}", error.message())
                        ));
                    }
                    _ => {}
                }
            }
        })
        .await
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::TimedOut,
                format!("snapshot fetch of {} timed out", entry.path)
            )
        })??;
        manifest::validate_page_length(data.len() as u64, page_len)?;
        file.write_all_at(offset, &data).await?;
        offset += data.len() as u64;
        if offset >= entry.len {
            break;
        }
    }
    file.fsync().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn temp_snapshot_root() -> String {
        mudu_sys::env_var::temp_dir()
            .join(format!(
                "repl-snapshot-test-{}",
                mudu_sys::random::uuid_v4()
            ))
            .to_string_lossy()
            .to_string()
    }

    fn write_version(
        data_dir: &str,
        worker_index: u32,
        lsn: u64,
        kv_entries: &[KvDumpEntry],
        relation_files: &[(&str, &[u8])],
        with_manifest: bool,
    ) -> ReplSnapshotManifest {
        let dir = snapshot_version_dir(data_dir, worker_index, lsn);
        mudu_sys::fs::sync::create_dir_all(dir.join(RELATION_DIR_NAME)).unwrap();
        let mut files = Vec::new();
        for (name, content) in relation_files {
            let rel = format!("{RELATION_DIR_NAME}/{name}");
            write_file_sync(&dir.join(&rel), content).unwrap();
            files.push(ReplSnapshotFileEntry {
                path: rel,
                len: content.len() as u64,
                crc64: crc64(content),
            });
        }
        let kv_bytes = rmp_serde::to_vec(&kv_entries.to_vec()).unwrap();
        write_file_sync(&dir.join(KV_FILE_NAME), &kv_bytes).unwrap();
        files.push(ReplSnapshotFileEntry {
            path: KV_FILE_NAME.to_string(),
            len: kv_bytes.len() as u64,
            crc64: crc64(&kv_bytes),
        });
        let image = recovery::RecoveryImage {
            version: 1,
            worker_index,
            wal_resume: lsn + 1,
            watermarks: xcontract::worker_storage::RecoveryWatermarks {
                max_xid: kv_entries.iter().map(|(_, ts, _)| *ts).max().unwrap_or(0),
                applied_epoch: 0,
            },
            participants: Default::default(),
        };
        let recovery_bytes = rmp_serde::to_vec(&image).unwrap();
        write_file_sync(&dir.join(recovery::RECOVERY_FILE), &recovery_bytes).unwrap();
        files.push(ReplSnapshotFileEntry {
            path: recovery::RECOVERY_FILE.into(),
            len: recovery_bytes.len() as u64,
            crc64: crc64(&recovery_bytes),
        });
        let manifest = ReplSnapshotManifest {
            version: 2,
            worker_index,
            committed_lsn: lsn,
            seg_hash: [3u8; 32],
            raft_term: 2,
            raft_index: 10,
            taken_at_ms: 1,
            files,
        };
        if with_manifest {
            let bytes = rmp_serde::to_vec(&manifest).unwrap();
            write_file_sync(&dir.join(MANIFEST_FILE_NAME), &bytes).unwrap();
        }
        manifest
    }

    #[test]
    fn manifest_loader_picks_highest_valid_version() {
        let data_dir = temp_snapshot_root();
        let m100 = write_version(&data_dir, 0, 100, &[], &[("0.1.0.dat", b"abc")], true);
        // an incomplete newer version (no manifest yet) is ignored
        let _ = write_version(&data_dir, 0, 200, &[], &[], false);
        let loaded = load_latest_manifest(&data_dir, 0).unwrap().unwrap();
        assert_eq!(loaded, m100);
        // completing the newer version makes it the latest
        let m200 = write_version(
            &data_dir,
            0,
            200,
            &[(b"k".to_vec(), 7, Some(b"v".to_vec()))],
            &[],
            true,
        );
        let loaded = load_latest_manifest(&data_dir, 0).unwrap().unwrap();
        assert_eq!(loaded, m200);
        assert_eq!(loaded.resume_lsn(), 201);
        // an unknown worker has no snapshot
        assert!(load_latest_manifest(&data_dir, 9).unwrap().is_none());
        // the kv dump round-trips
        let kv = read_kv_dump(&snapshot_version_dir(&data_dir, 0, 200)).unwrap();
        assert_eq!(kv, vec![(b"k".to_vec(), 7u64, Some(b"v".to_vec()))]);
    }

    #[test]
    fn verify_detects_corrupted_files() {
        let data_dir = temp_snapshot_root();
        let manifest = write_version(&data_dir, 0, 100, &[], &[("0.1.0.dat", b"abc")], true);
        let dir = snapshot_version_dir(&data_dir, 0, 100);
        verify_snapshot_files(&dir, &manifest).unwrap();
        // flip a byte: the CRC64 no longer matches
        mudu_sys::fs::sync::write(dir.join(RELATION_DIR_NAME).join("0.1.0.dat"), b"abd").unwrap();
        assert!(verify_snapshot_files(&dir, &manifest).is_err());
        // a missing file fails as well
        let manifest_missing = write_version(&data_dir, 1, 100, &[], &[("0.2.0.dat", b"x")], true);
        let dir1 = snapshot_version_dir(&data_dir, 1, 100);
        mudu_sys::fs::sync::sync_remove_dir_all(dir1.join(RELATION_DIR_NAME)).unwrap();
        assert!(verify_snapshot_files(&dir1, &manifest_missing).is_err());
        // A sealed snapshot missing bytes fails closed; it may already protect GC.
        assert!(load_latest_manifest(&data_dir, 1).is_err());
    }

    #[test]
    fn invalid_manifest_file_sets_are_rejected() {
        assert!(manifest::validate_page_length(0, 1).is_err());
        assert!(manifest::validate_page_length(2, 1).is_err());
        assert!(manifest::validate_page_length(0, 0).is_ok());
        assert!(manifest::validate_page_length(1, 2).is_ok());
        let root = temp_snapshot_root();
        let original = write_version(&root, 0, 1, &[], &[], false);
        for path in [
            "../kv.mp",
            "/kv.mp",
            "relation/../x.dat",
            "relation//x.dat",
            "relation/./x.dat",
            "relation\\x.dat",
            "manifest.mp",
            "unknown.mp",
        ] {
            let mut manifest = original.clone();
            manifest.files[0].path = path.to_string();
            assert!(validate_snapshot_manifest(&manifest).is_err(), "{path}");
        }
        let mut manifest = original.clone();
        manifest.files.push(manifest.files[0].clone());
        assert!(validate_snapshot_manifest(&manifest).is_err());
        manifest = original.clone();
        manifest.files.clear();
        assert!(validate_snapshot_manifest(&manifest).is_err());
        manifest = original.clone();
        manifest.version = 99;
        assert!(validate_snapshot_manifest(&manifest).is_err());
        manifest = original;
        manifest.committed_lsn = u64::MAX;
        assert!(validate_snapshot_manifest(&manifest).is_err());
    }

    #[test]
    fn sealed_corruption_never_falls_back_to_an_older_cut() {
        let root = temp_snapshot_root();
        write_version(&root, 0, 1, &[], &[], true);
        write_version(&root, 0, 2, &[], &[("x.dat", b"abc")], true);
        let newer = snapshot_version_dir(&root, 0, 2);
        mudu_sys::fs::sync::write(newer.join("relation/x.dat"), b"abd").unwrap();
        assert!(load_latest_manifest(&root, 0).is_err());
        // Same length does not make corrupted bytes a recoverable snapshot.
        assert!(manifest::reusable_snapshot(&newer, 0, 2, [3; 32]).is_err());
    }

    #[test]
    fn failed_seal_leaves_no_manifest_and_retry_publishes_verified_files() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let root = temp_snapshot_root();
            let manifest = write_version(&root, 0, 1, &[], &[("x.dat", b"abc")], false);
            let dir = snapshot_version_dir(&root, 0, 1);
            mudu_sys::fs::sync::write(dir.join("relation/x.dat"), b"abd").unwrap();
            assert!(seal_snapshot(&dir, &manifest).await.is_err());
            assert!(!mudu_sys::fs::sync::sync_path_exists(
                dir.join(MANIFEST_FILE_NAME)
            ));
            assert!(load_latest_manifest(&root, 0).unwrap().is_none());
            mudu_sys::fs::sync::write(dir.join("relation/x.dat"), b"abc").unwrap();
            seal_snapshot(&dir, &manifest).await.unwrap();
            assert_eq!(
                load_latest_manifest(&root, 0).unwrap(),
                Some(manifest.clone())
            );
            let mut replacement = manifest.clone();
            replacement.committed_lsn = 2;
            assert!(seal_snapshot(&dir, &replacement).await.is_err());
            assert_eq!(load_latest_manifest(&root, 0).unwrap(), Some(manifest));
            assert!(manifest::reusable_snapshot(&dir, 1, 1, [3; 32]).is_err());
            assert!(manifest::reusable_snapshot(&dir, 0, 1, [4; 32]).is_err());
        })
        .unwrap();
    }

    #[test]
    fn restore_checks_every_file_before_replacing_any_live_relation() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let root = temp_snapshot_root();
            let manifest = write_version(
                &root,
                0,
                1,
                &[],
                &[("x.dat", b"new"), ("y.dat", b"abc")],
                true,
            );
            let dir = snapshot_version_dir(&root, 0, 1);
            let target = temp_snapshot_root();
            mudu_sys::fs::sync::create_dir_all(Path::new(&target).join("relation")).unwrap();
            let live = Path::new(&target).join("relation/x.dat");
            mudu_sys::fs::sync::write(&live, b"old").unwrap();
            mudu_sys::fs::sync::write(dir.join("relation/y.dat"), b"abd").unwrap();
            assert!(install_snapshot_relation_files(&target, &dir, &manifest)
                .await
                .is_err());
            assert_eq!(mudu_sys::fs::sync::read(&live).unwrap(), b"old");
        })
        .unwrap();
    }
    #[test]
    fn recovery_image_rejects_foreign_cut_and_insufficient_clocks() {
        let root = temp_snapshot_root();
        let manifest = write_version(
            &root,
            0,
            7,
            &[(b"k".to_vec(), 9, Some(b"v".to_vec()))],
            &[],
            true,
        );
        let dir = snapshot_version_dir(&root, 0, 7);
        let original = mudu_sys::fs::sync::read(dir.join(recovery::RECOVERY_FILE)).unwrap();
        for kind in 0..4 {
            let mut image: recovery::RecoveryImage = rmp_serde::from_slice(&original).unwrap();
            match kind {
                0 => image.worker_index = 1,
                1 => image.wal_resume += 1,
                2 => image.watermarks.max_xid = 8,
                _ => image.version = 99,
            }
            let bytes = rmp_serde::to_vec(&image).unwrap();
            write_file_sync(&dir.join(recovery::RECOVERY_FILE), &bytes).unwrap();
            let mut amended = manifest.clone();
            let entry = amended
                .files
                .iter_mut()
                .find(|e| e.path == recovery::RECOVERY_FILE)
                .unwrap();
            entry.len = bytes.len() as u64;
            entry.crc64 = crc64(&bytes);
            // CRC alone is insufficient: even a correctly checksummed image
            // must match the manifest cut and cover the stored row clocks.
            assert!(verify_snapshot_files(&dir, &amended).is_err());
        }
        let mut old = manifest;
        old.version = 1;
        assert!(validate_snapshot_manifest(&old).is_err());
    }
}
