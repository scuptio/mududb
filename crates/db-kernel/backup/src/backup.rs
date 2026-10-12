//! WAL archiving for backup / PITR: a periodic per-worker task that copies
//! sealed WAL chunks to the archive directory and advances the persisted
//! archive frontier. It runs independently of replication so single-node
//! deployments archive too; the archive frontier is what lets WAL prefix GC
//! reclaim a chunk only after it has been durably archived.
//!
//! Also the base-backup orchestration (P3): a backup label binds one
//! per-worker snapshot (at its committed point) together with the WAL archive
//! range needed to replay forward from there.

use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::repl::seg_hash::{SegHash, ZERO_SEG_HASH};
use ::repl::snapshot::{snapshot_version_dir, take_worker_snapshot};
use ::worker::worker::WorkerRuntime;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::sync::SMutex;
use mudu_sys::tokio;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tracing::{debug, warn};

/// Interval between archive sweeps. Matches the coarse GC cadence; archiving
/// is cheap (sealed chunks only), so it can run more often than GC.
pub(crate) const ARCHIVE_CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// The backup-label file name inside a backup directory.
pub(crate) const BACKUP_LABEL_FILE: &str = "BACKUP_LABEL";

/// One worker's position inside a base backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupWorkerEntry {
    /// Worker index within the server.
    pub worker_index: u32,
    /// Worker id (`log_oid`): the restore derives the archived chunk
    /// filename (`short_oid`) and the worker registry marker from it.
    pub worker_id: OID,
    /// Partitions hosted by the worker (relation file ownership + registry).
    pub partition_ids: Vec<OID>,
    /// The committed LSN the worker's snapshot covers.
    pub base_lsn: u64,
    /// Segment hash at `base_lsn`.
    pub seg_hash: SegHash,
}

/// The self-sealing manifest of a base backup (PG `backup_label`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupLabel {
    /// Backup label (the directory name under `backup_dir`).
    pub label: String,
    /// Timeline the backup belongs to (single-timeline in v1).
    pub timeline: u64,
    /// Wall-clock time the backup was taken (millis since the epoch).
    pub taken_at_ms: u64,
    /// Per-worker snapshot positions.
    pub workers: Vec<BackupWorkerEntry>,
}

/// Runs the WAL archiver of one worker until `stop_rx` fires. When the
/// worker's log has no archive directory configured, this is a no-op.
pub async fn run_worker_archive_loop(worker: WorkerRuntime, mut stop_rx: StopRx) -> RS<()> {
    let Some(log) = worker.worker_log()? else {
        debug!("archive loop: worker has no log, not starting");
        return Ok(());
    };
    let layout = log.layout()?;
    if layout.archive_dir().is_none() {
        return Ok(());
    }
    let fs = log.fs();
    loop {
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(ARCHIVE_CHECK_INTERVAL) => {}
            _ = stop_rx.changed() => {
                return Ok(());
            }
        }
        if stop_rx.is_stopped() {
            return Ok(());
        }
        match layout.archive_prefix_chunks(fs.as_ref()).await {
            Ok((bytes, chunks)) => {
                if chunks > 0 {
                    debug!(bytes, chunks, "worker WAL archived");
                }
            }
            Err(err) => {
                warn!("worker WAL archive failed: {err}");
            }
        }
    }
}

/// The backup directory of one label.
fn backup_label_dir(backup_dir: &str, label: &str) -> PathBuf {
    PathBuf::from(backup_dir).join(label)
}

/// The per-worker snapshot directory inside a backup.
fn backup_worker_dir(backup_dir: &str, label: &str, worker_index: u32) -> PathBuf {
    backup_label_dir(backup_dir, label).join(format!("worker_{worker_index}"))
}

/// Takes a base backup: snapshots every listed worker at its committed point,
/// copies the snapshot version into `backup/<label>/`, and publishes the
/// `BACKUP_LABEL` last (a backup is only valid once its label decodes and
/// every referenced snapshot is complete).
///
/// The caller must quiesce/force-flush each worker's WAL before calling so the
/// snapshot's `committed_lsn` reflects a durable, applied point (the snapshot
/// routine itself flushes dirty pages and fsyncs the data files).
pub(crate) async fn run_base_backup(
    workers: &[(&WorkerRuntime, u32)],
    backup_dir: &str,
    label: &str,
) -> RS<BackupLabel> {
    let mut entries = Vec::with_capacity(workers.len());
    let taken_at_ms = mudu_sys::time::system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    for (worker, worker_index) in workers {
        let committed_lsn = worker
            .worker_log()?
            .map(|log| log.last_allocated_lsn().as_u64())
            .unwrap_or(0);
        let manifest =
            take_worker_snapshot(worker, *worker_index, committed_lsn, ZERO_SEG_HASH, 0, 0).await?;
        copy_snapshot_into_backup(
            worker,
            *worker_index,
            committed_lsn,
            backup_dir,
            label,
            &manifest,
        )?;
        entries.push(BackupWorkerEntry {
            worker_index: *worker_index,
            worker_id: worker.worker_id(),
            partition_ids: worker.partition_ids().to_vec(),
            base_lsn: committed_lsn,
            seg_hash: manifest.seg_hash,
        });
    }
    let backup_label = BackupLabel {
        label: label.to_string(),
        timeline: 1,
        taken_at_ms,
        workers: entries,
    };
    write_backup_label(backup_dir, &backup_label)?;
    Ok(backup_label)
}

/// Copies the live snapshot version (just produced by
/// [`take_worker_snapshot`]) into the backup directory, writing the manifest
/// last so an incomplete copy is never treated as a valid backup.
fn copy_snapshot_into_backup(
    worker: &WorkerRuntime,
    worker_index: u32,
    committed_lsn: u64,
    backup_dir: &str,
    label: &str,
    manifest: &mudu_contract::protocol::ReplSnapshotManifest,
) -> RS<()> {
    let src_dir = snapshot_version_dir(
        worker.storage().relation_path(),
        worker_index,
        committed_lsn,
    );
    let dst_dir =
        backup_worker_dir(backup_dir, label, worker_index).join(format!("v{committed_lsn}"));
    if mudu_sys::fs::sync::sync_path_exists(&dst_dir) {
        mudu_sys::fs::sync::sync_remove_dir_all(&dst_dir)?;
    }
    for entry in &manifest.files {
        let src = src_dir.join(&entry.path);
        let dst = dst_dir.join(&entry.path);
        if let Some(parent) = dst.parent() {
            mudu_sys::fs::sync::create_dir_all(parent)?;
        }
        let bytes = mudu_sys::fs::sync::read(&src)?;
        if bytes.len() as u64 != entry.len {
            return Err(mudu_error!(
                ErrorCode::Io,
                format!(
                    "snapshot file {} length mismatch while copying backup",
                    entry.path
                )
            ));
        }
        write_file_sync(&dst, &bytes)?;
    }
    // Manifest last, mirroring the snapshot publish rule.
    let manifest_bytes = rmp_serde::to_vec(manifest).map_err(|e| {
        mudu_error!(
            ErrorCode::Encode,
            "encode backup snapshot manifest error",
            e
        )
    })?;
    write_file_sync(&dst_dir.join("manifest.mp"), &manifest_bytes)?;
    Ok(())
}

/// Serializes and writes the backup label to `backup/<label>/BACKUP_LABEL`.
fn write_backup_label(backup_dir: &str, label: &BackupLabel) -> RS<()> {
    let dir = backup_label_dir(backup_dir, &label.label);
    mudu_sys::fs::sync::create_dir_all(&dir)?;
    let bytes = rmp_serde::to_vec(label)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode backup label error", e))?;
    write_file_sync(&dir.join(BACKUP_LABEL_FILE), &bytes)?;
    Ok(())
}

/// Loads a backup label, `None` when it does not exist or does not decode.
pub(crate) fn load_backup_label(backup_dir: &str, label: &str) -> RS<Option<BackupLabel>> {
    let path = backup_label_dir(backup_dir, label).join(BACKUP_LABEL_FILE);
    if !mudu_sys::fs::sync::sync_path_exists(&path) {
        return Ok(None);
    }
    let bytes = mudu_sys::fs::sync::read(&path)?;
    match rmp_serde::from_slice::<BackupLabel>(&bytes) {
        Ok(label) => Ok(Some(label)),
        Err(_) => Ok(None),
    }
}

/// Writes `bytes` to `path` synchronously with an fsync.
fn write_file_sync(path: &std::path::Path, bytes: &[u8]) -> RS<()> {
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = options.open(path)?;
    file.write_all(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "write backup file error", e))?;
    file.sync_data()?;
    Ok(())
}

/// A handle to a worker registered for backup (any server mode).
struct BackupWorkerHandle {
    worker: Arc<WorkerRuntime>,
    worker_index: u32,
}

fn backup_worker_registry() -> &'static SMutex<BTreeMap<(ServerInstanceId, u32), BackupWorkerHandle>>
{
    static REGISTRY: OnceLock<SMutex<BTreeMap<(ServerInstanceId, u32), BackupWorkerHandle>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(BTreeMap::new()))
}

/// Registers a worker under its index so the backup endpoint can reach every
/// live worker of the server (replication and single-node alike).
pub fn register_backup_worker(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    worker: Arc<WorkerRuntime>,
) -> RS<()> {
    let mut registry = backup_worker_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "backup worker registry lock poisoned"))?;
    registry.insert(
        (server_instance_id, worker_index),
        BackupWorkerHandle {
            worker,
            worker_index,
        },
    );
    Ok(())
}

/// Removes a worker from the backup registry; unknown entries are ignored so
/// shutdown stays idempotent.
pub fn unregister_backup_worker(server_instance_id: ServerInstanceId, worker_index: u32) -> RS<()> {
    let mut registry = backup_worker_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "backup worker registry lock poisoned"))?;
    registry.remove(&(server_instance_id, worker_index));
    Ok(())
}

/// Snapshots the live workers of the server, ordered by worker index.
pub(crate) fn backup_workers(
    server_instance_id: ServerInstanceId,
) -> RS<Vec<(Arc<WorkerRuntime>, u32)>> {
    let registry = backup_worker_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "backup worker registry lock poisoned"))?;
    let mut workers: Vec<(Arc<WorkerRuntime>, u32)> = registry
        .iter()
        .filter(|((instance, _), _)| *instance == server_instance_id)
        .map(|(_, handle)| (handle.worker.clone(), handle.worker_index))
        .collect();
    workers.sort_by_key(|(_, index)| *index);
    Ok(workers)
}

/// Takes a base backup of the whole server: force-flushes every worker's WAL
/// (so the snapshot point is durable), snapshots each worker at its committed
/// point, copies the snapshots under `backup/<label>/`, and publishes the
/// `BACKUP_LABEL` last. Runs against the live workers of `server_instance_id`.
pub async fn run_server_base_backup(
    server_instance_id: ServerInstanceId,
    backup_dir: &str,
    label: &str,
) -> RS<BackupLabel> {
    let workers = backup_workers(server_instance_id)?;
    if workers.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            format!(
                "server {server_instance_id} has no registered workers; cannot take a base backup"
            )
        ));
    }
    // Quiesce: force-flush each worker's WAL so the snapshot point covers a
    // durable, applied prefix. The snapshot routine itself flushes dirty data
    // pages and fsyncs the relation files.
    for (worker, _) in &workers {
        if let Some(log) = worker.worker_log()? {
            log.force_flush_log_async().await?;
        }
    }
    let refs: Vec<(&WorkerRuntime, u32)> = workers
        .iter()
        .map(|(worker, index)| (worker.as_ref(), *index))
        .collect();
    run_base_backup(&refs, backup_dir, label).await
}

/// Lists the labels of every valid backup under `backup_dir`.
pub fn list_backup_labels(backup_dir: &str) -> RS<Vec<String>> {
    let mut labels = Vec::new();
    if !mudu_sys::fs::sync::sync_path_exists(backup_dir) {
        return Ok(labels);
    }
    for path in mudu_sys::fs::sync::read_dir(backup_dir)? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if load_backup_label(backup_dir, name)?.is_some() {
            labels.push(name.to_string());
        }
    }
    labels.sort();
    Ok(labels)
}

/// Verifies one backup: decodes the label and checks every referenced
/// snapshot file against its manifest (length + CRC64). Returns the label on
/// success; a missing or corrupt backup yields `None`.
pub fn verify_backup(backup_dir: &str, label: &str) -> RS<Option<BackupLabel>> {
    let Some(label) = load_backup_label(backup_dir, label)? else {
        return Ok(None);
    };
    for entry in &label.workers {
        let worker_dir = backup_worker_dir(backup_dir, &label.label, entry.worker_index);
        let manifest_path = worker_dir
            .join(format!("v{}", entry.base_lsn))
            .join("manifest.mp");
        if !mudu_sys::fs::sync::sync_path_exists(&manifest_path) {
            return Ok(None);
        }
        let bytes = mudu_sys::fs::sync::read(&manifest_path)?;
        let manifest: mudu_contract::protocol::ReplSnapshotManifest =
            match rmp_serde::from_slice(&bytes) {
                Ok(manifest) => manifest,
                Err(_) => return Ok(None),
            };
        let snapshot_dir = worker_dir.join(format!("v{}", entry.base_lsn));
        if let Err(err) = ::repl::snapshot::verify_snapshot_files(&snapshot_dir, &manifest) {
            warn!(
                label = %label.label,
                worker_index = entry.worker_index,
                "backup verify failed: {err}"
            );
            return Ok(None);
        }
    }
    Ok(Some(label))
}

/// Deletes one backup directory (`backup/<label>/`), ignoring a missing
/// label.
pub fn delete_backup(backup_dir: &str, label: &str) -> RS<bool> {
    let dir = backup_label_dir(backup_dir, label);
    if !mudu_sys::fs::sync::sync_path_exists(&dir) {
        return Ok(false);
    }
    mudu_sys::fs::sync::sync_remove_dir_all(&dir)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use mudu_sys::env_var::temp_dir;

    #[test]
    fn backup_label_round_trips_and_seals() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backup_dir = temp_dir()
                .join(format!("mudu_backup_label_{}", mudu_utils::oid::gen_oid()))
                .to_string_lossy()
                .to_string();

            // Empty-worker backup still publishes a valid, loadable label.
            let label = run_base_backup(&[], &backup_dir, "test").await.unwrap();
            assert_eq!(label.label, "test");
            assert_eq!(label.timeline, 1);
            assert!(label.workers.is_empty());

            let loaded = load_backup_label(&backup_dir, "test").unwrap().unwrap();
            assert_eq!(loaded, label);

            // A missing label is None, not an error.
            assert!(load_backup_label(&backup_dir, "absent").unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn backup_label_corrupt_is_ignored() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backup_dir = temp_dir()
                .join(format!(
                    "mudu_backup_corrupt_{}",
                    mudu_utils::oid::gen_oid()
                ))
                .to_string_lossy()
                .to_string();
            let _ = run_base_backup(&[], &backup_dir, "test").await.unwrap();
            let label_path = backup_label_dir(&backup_dir, "test").join(BACKUP_LABEL_FILE);
            // Corrupt the label: the loader reports None rather than erroring.
            write_file_sync(&label_path, b"not-a-label").unwrap();
            assert!(load_backup_label(&backup_dir, "test").unwrap().is_none());
        })
        .unwrap()
    }
}
