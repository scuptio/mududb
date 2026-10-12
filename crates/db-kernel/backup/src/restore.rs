//! Offline restore / point-in-time recovery.
//!
//! Reconstructs a usable server data directory from a base backup plus the
//! continuously-archived WAL: the base backup supplies the relation files and
//! the in-memory KV store state at its `base_lsn`, and the archived WAL is
//! replayed forward to a target point (`EOF` for a full restore, or a
//! `--lsn` / `--time` / `--xid` stop point for PITR).
//!
//! The replay reuses the worker's storage replay path (`replay_log_batch`),
//! so relation pages are applied idempotently (page LSN stamps) and the KV
//! store is brought forward exactly like a crash-recovery replay. The
//! restored KV is then written back into the target's own WAL so a
//! single-node server started on the target directory re-derives it on its
//! next recovery, and the final state is sealed into a snapshot so a
//! replication-mode server can also resume from it.

use crate::backup::load_backup_label;
use ::repl::snapshot::{install_snapshot_relation_files, read_kv_dump};
use ::wal::commit_ts::CommitTimestamp;
use ::wal::log_frame::decode_entries_with_pending;
use ::wal::worker_log::{scan_valid_frame_prefix, WalSyncPolicy, WorkerLogBatching};
use ::wal::xl_batch::{append_xl_batch_async, XLBatch};
use ::wal::xl_data_op::{XLDelete, XLInsert, XLWrite};
use ::wal::xl_entry::{TxOp, XLEntry};
use ::worker::server_cfg::ReplRole;
use ::worker::worker::{WorkerRuntime, WorkerRuntimeParams};
use ::worker::worker_registry::{WorkerIdentity, WorkerRegistry};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::ReplSnapshotManifest;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tracing::warn;

/// Where recovery stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreTarget {
    /// Replay to the end of the archived WAL.
    End,
    /// Stop before the given LSN.
    Lsn(u64),
    /// Stop after the last commit whose timestamp is `<=` the given millis.
    Time(u64),
    /// Stop after the commit of the given transaction id.
    Xid(u64),
}

/// One worker's restore outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkerRestoreReport {
    /// Worker index.
    pub worker_index: u32,
    /// Worker id.
    pub worker_id: OID,
    /// The base backup's committed LSN.
    pub base_lsn: u64,
    /// The LSN recovery stopped at (EOF high water or the target LSN).
    pub target_lsn: u64,
    /// Number of WAL batches replayed.
    pub replayed_batches: u64,
    /// Last commit timestamp observed up to the recovery point.
    pub last_commit_ts: Option<u64>,
}

/// The full restore outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    /// The backup label restored.
    pub label: String,
    /// Per-worker reports.
    pub workers: Vec<WorkerRestoreReport>,
}

/// The archived chunk file prefix for a worker (its `short_oid`).
fn short_oid_of(worker_id: OID) -> String {
    use short_uuid::ShortUuid;
    use uuid::Uuid;
    ShortUuid::from_uuid(&Uuid::from_u128(worker_id)).to_string()
}

/// The per-worker snapshot directory inside a backup.
fn backup_worker_snapshot_dir(
    backup_dir: &str,
    label: &str,
    worker_index: u32,
    base_lsn: u64,
) -> std::path::PathBuf {
    std::path::PathBuf::from(backup_dir)
        .join(label)
        .join(format!("worker_{worker_index}"))
        .join(format!("v{base_lsn}"))
}

/// Loads the snapshot manifest of a backup worker.
fn load_backup_manifest(
    backup_dir: &str,
    label: &str,
    worker_index: u32,
    base_lsn: u64,
) -> RS<ReplSnapshotManifest> {
    let dir = backup_worker_snapshot_dir(backup_dir, label, worker_index, base_lsn);
    let bytes = mudu_sys::fs::sync::read(dir.join("manifest.mp"))?;
    rmp_serde::from_slice(&bytes).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            "decode backup snapshot manifest error",
            e
        )
    })
}

/// Sorted archived chunk paths of one worker, plus the `(lsn, seq)` pairs.
fn archived_chunks(archive_dir: &str, worker_id: OID) -> RS<Vec<std::path::PathBuf>> {
    let prefix = format!("{}.", short_oid_of(worker_id));
    let mut entries: Vec<(u64, std::path::PathBuf)> = Vec::new();
    for path in mudu_sys::fs::sync::read_dir(archive_dir)? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".xl") {
            continue;
        }
        let seq_text = &name[prefix.len()..name.len() - 3];
        if let Ok(seq) = seq_text.parse::<u64>() {
            entries.push((seq, path));
        }
    }
    entries.sort_by_key(|(seq, _)| *seq);
    Ok(entries.into_iter().map(|(_, path)| path).collect())
}

/// Decodes the WAL batches of one chunk into `(start_lsn, batch)` pairs.
fn decode_chunk_batches(
    path: &Path,
    pending: &mut Vec<Vec<u8>>,
    pending_start: &mut Option<::wal::lsn::LSN>,
) -> RS<Vec<(u64, XLBatch)>> {
    let bytes = mudu_sys::fs::sync::read(path)?;
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let prefix = scan_valid_frame_prefix(&bytes);
    if prefix.corrupt_reason.is_some() {
        warn!(path = %path.display(), "restore: dropping corrupt archived chunk tail");
    }
    let decoded = decode_entries_with_pending::<XLBatch>(&prefix.frames, pending, pending_start)?;
    Ok(decoded
        .into_iter()
        .map(|(lsn, batch)| (lsn.as_u64(), batch))
        .collect())
}

/// The highest commit timestamp in a batch, if any.
fn batch_commit_ts(batch: &XLBatch) -> Option<CommitTimestamp> {
    batch
        .entries
        .iter()
        .flat_map(|entry| entry.ops.iter())
        .filter_map(TxOp::commit_ts)
        .max()
}

/// Resolves the exclusive stop LSN for a target by scanning the archive: the
/// target is translated to an LSN (exclusive upper bound). `End` resolves to
/// the archive's high-water LSN (nothing is skipped).
fn resolve_target_lsn(
    archive_dir: &str,
    worker_id: OID,
    base_lsn: u64,
    target: RestoreTarget,
) -> RS<u64> {
    if let RestoreTarget::Lsn(lsn) = target {
        return Ok(lsn);
    }
    let mut pending = Vec::new();
    let mut pending_start = None;
    // Track the start LSN of the batch that satisfied the target so far.
    let mut stop_lsn: Option<u64> = None;
    let mut eof_lsn = base_lsn;
    for path in archived_chunks(archive_dir, worker_id)? {
        for (start_lsn, batch) in decode_chunk_batches(&path, &mut pending, &mut pending_start)? {
            eof_lsn = eof_lsn.max(start_lsn + 1);
            let matched = match target {
                RestoreTarget::Time(time) => {
                    batch_commit_ts(&batch).is_some_and(|ts| ts.as_millis() <= time)
                }
                RestoreTarget::Xid(xid) => batch.entries.iter().any(|entry| {
                    entry.xid == xid && entry.ops.iter().any(|op| op.commit_ts().is_some())
                }),
                RestoreTarget::End | RestoreTarget::Lsn(_) => false,
            };
            if matched {
                stop_lsn = Some(start_lsn + 1);
            }
        }
    }
    match target {
        RestoreTarget::End => Ok(eof_lsn),
        RestoreTarget::Lsn(_) => unreachable!("handled above"),
        RestoreTarget::Time(_) | RestoreTarget::Xid(_) => stop_lsn.ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                "no commit in the archive satisfies the recovery target"
            )
        }),
    }
}

/// Replays the archived WAL of one worker into `worker`, stopping before the
/// exclusive `target_lsn`. Returns `(replayed_batches, last_commit_ts,
/// eof_lsn)`.
async fn replay_archived_wal(
    worker: &WorkerRuntime,
    archive_dir: &str,
    worker_id: OID,
    base_lsn: u64,
    target_lsn: u64,
) -> RS<(u64, Option<u64>, u64)> {
    let mut pending = Vec::new();
    let mut pending_start = None;
    let mut replayed = 0u64;
    let mut last_commit_ts: Option<u64> = None;
    let mut eof_lsn = base_lsn;
    for path in archived_chunks(archive_dir, worker_id)? {
        for (start_lsn, batch) in decode_chunk_batches(&path, &mut pending, &mut pending_start)? {
            eof_lsn = eof_lsn.max(start_lsn + 1);
            if start_lsn >= target_lsn {
                continue;
            }
            if let Some(ts) = batch_commit_ts(&batch) {
                last_commit_ts =
                    Some(last_commit_ts.map_or(ts.as_millis(), |c| c.max(ts.as_millis())));
            }
            worker.replay_log_batch(batch).await?;
            replayed += 1;
        }
    }
    Ok((replayed, last_commit_ts, eof_lsn))
}

/// Writes the worker's current KV store back into its WAL as fresh batches so
/// a single-node restart re-derives it on recovery (the KV store has no
/// separate durable checkpoint).
async fn persist_kv_to_wal(worker: &WorkerRuntime) -> RS<()> {
    let Some(log) = worker.worker_log()? else {
        return Ok(());
    };
    let entries = worker.storage().dump_kv_latest();
    // Chunk the dump into bounded batches so a huge KV store does not become
    // one giant WAL entry.
    const BATCH_KEYS: usize = 512;
    for chunk in entries.chunks(BATCH_KEYS) {
        let mut ops = Vec::with_capacity(chunk.len() + 2);
        ops.push(TxOp::Begin);
        for (key, _, value) in chunk {
            ops.push(TxOp::Write(match value {
                Some(value) => XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: key.clone(),
                    value: value.clone(),
                }),
                None => XLWrite::Delete(XLDelete {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: key.clone(),
                }),
            }));
        }
        ops.push(TxOp::Commit(CommitTimestamp::ZERO));
        append_xl_batch_async(&log, &XLBatch::new(vec![XLEntry { xid: 0, ops }])).await?;
    }
    log.flush_async().await?;
    Ok(())
}

/// Restores one worker of a backup into `target_dir`.
async fn restore_worker(
    backup_dir: &str,
    archive_dir: &str,
    label: &str,
    target_dir: &str,
    worker_count: usize,
    entry: &crate::backup::BackupWorkerEntry,
    target: RestoreTarget,
) -> RS<WorkerRestoreReport> {
    let snapshot_dir =
        backup_worker_snapshot_dir(backup_dir, label, entry.worker_index, entry.base_lsn);
    let manifest = load_backup_manifest(backup_dir, label, entry.worker_index, entry.base_lsn)?;
    // 1. Install the base backup relation files into the target data dir.
    install_snapshot_relation_files(target_dir, &snapshot_dir, &manifest).await?;

    // 2. Resolve the stop LSN, then seed the target WAL allocator at that LSN
    // so the restored server continues the timeline instead of restarting at
    // zero (relation pages carry LSN stamps from the source timeline).
    let target_lsn = resolve_target_lsn(archive_dir, entry.worker_id, entry.base_lsn, target)?;
    seed_gc_frontier(target_dir, entry.worker_id, target_lsn)?;

    // 3. Build a headless worker over the target directory.
    let identity = WorkerIdentity {
        worker_index: entry.worker_index as usize,
        worker_id: entry.worker_id,
        partition_ids: entry.partition_ids.clone(),
    };
    let registry = Arc::new(WorkerRegistry::new(vec![identity.clone()])?);
    let worker = WorkerRuntime::new(WorkerRuntimeParams {
        identity,
        worker_count,
        log_dir: target_dir.to_string(),
        data_dir: target_dir.to_string(),
        log_chunk_size: 64 * 1024 * 1024,
        log_batching: WorkerLogBatching::default(),
        wal_sync_policy: WalSyncPolicy::Commit,
        procedure_runtime: None,
        registry,
        async_runtime: None,
        server_instance_id: mudu_utils::oid::gen_oid(),
        repl_role: ReplRole::Primary,
        repl_leader_hint: None,
        raft_share: None,
        archive_dir: None,
    })
    .await?;
    worker.initialize().await?;
    worker.bootstrap_storage_async().await?;
    // 4. Seed the in-memory KV store from the base backup dump.
    let kv = read_kv_dump(&snapshot_dir)?;
    worker.storage().restore_kv_latest(kv).await?;

    // 5. Replay the archived WAL up to the resolved target.
    let (replayed_batches, last_commit_ts, _eof_lsn) = replay_archived_wal(
        &worker,
        archive_dir,
        entry.worker_id,
        entry.base_lsn,
        target_lsn,
    )
    .await?;

    // 6. Persist the restored KV into the target's own WAL (continuing from
    // `target_lsn`), flush the data pages, and finish recovery so the target
    // is a valid server directory.
    persist_kv_to_wal(&worker).await?;
    ::worker::worker::flush_worker_dirty_pages(&worker).await?;
    worker.finish_log_recovery()?;

    Ok(WorkerRestoreReport {
        worker_index: entry.worker_index,
        worker_id: entry.worker_id,
        base_lsn: entry.base_lsn,
        target_lsn,
        replayed_batches,
        last_commit_ts,
    })
}

/// Writes the GC frontier of the target worker so its WAL allocator resumes
/// at `lsn` (the restored timeline high-water mark). Written before the
/// worker is created so its startup scan observes the frontier.
fn seed_gc_frontier(target_dir: &str, worker_id: OID, lsn: u64) -> RS<()> {
    use ::wal::worker_log::GcFrontier;
    let path =
        std::path::Path::new(target_dir).join(format!("{}.gcfrontier", short_oid_of(worker_id)));
    let bytes = rmp_serde::to_vec(&GcFrontier {
        lsn,
        first_alive_seq: 0,
    })
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode restored GC frontier error", e))?;
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = options.open(path)?;
    std::io::Write::write_all(&mut file, &bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "write restored GC frontier error", e))?;
    file.sync_data()?;
    Ok(())
}

/// Restores a base backup (plus archived WAL) into `target_dir`, stopping at
/// `target`. Returns a per-worker report. `backup_dir` and `archive_dir` are
/// the directories `[backup] backup_dir` / `archive_dir` were configured with.
pub async fn restore_backup(
    backup_dir: &str,
    archive_dir: &str,
    label: &str,
    target_dir: &str,
    target: RestoreTarget,
) -> RS<RestoreReport> {
    let Some(backup) = load_backup_label(backup_dir, label)? else {
        return Err(mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such backup {label}")
        ));
    };
    if backup.workers.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("backup {label} has no workers")
        ));
    }
    mudu_sys::fs::sync::create_dir_all(target_dir)?;
    let worker_count = backup.workers.len();
    let mut reports = Vec::with_capacity(worker_count);
    for entry in &backup.workers {
        let report = restore_worker(
            backup_dir,
            archive_dir,
            &backup.label,
            target_dir,
            worker_count,
            entry,
            target,
        )
        .await?;
        reports.push(report);
    }
    reports.sort_by_key(|report| report.worker_index);
    Ok(RestoreReport {
        label: backup.label,
        workers: reports,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use ::wal::xl_batch_worker_log::serialize_batch;
    use ::wal::xl_data_op::XLInsert;
    use std::sync::atomic::AtomicU64;

    fn write_chunk(
        archive_dir: &str,
        worker_id: OID,
        seq: u64,
        next_lsn: &AtomicU64,
        commit_ts: u64,
    ) {
        let batch = XLBatch::new(vec![XLEntry {
            xid: seq + 1,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k".to_vec(),
                    value: b"v".to_vec(),
                })),
                TxOp::Commit(CommitTimestamp(commit_ts)),
            ],
        }]);
        let frames = serialize_batch(&batch, 4096, next_lsn).unwrap();
        let mut bytes = Vec::new();
        for frame in frames {
            bytes.extend_from_slice(&frame);
        }
        let path = std::path::Path::new(archive_dir).join(format!(
            "{}.{}.xl",
            short_oid_of(worker_id),
            seq
        ));
        let mut options = mudu_sys::fs::sync::SOpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut file = options.open(path).unwrap();
        std::io::Write::write_all(&mut file, &bytes).unwrap();
        file.sync_data().unwrap();
    }

    #[test]
    fn resolve_target_time_stops_after_last_commit_before_time() {
        let archive_dir = mudu_sys::env_var::temp_dir()
            .join(format!(
                "mudu_restore_target_{}",
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .to_string();
        mudu_sys::fs::sync::create_dir_all(&archive_dir).unwrap();
        let worker_id = 1u128;
        let next_lsn = AtomicU64::new(0);
        write_chunk(&archive_dir, worker_id, 0, &next_lsn, 1000);
        write_chunk(&archive_dir, worker_id, 1, &next_lsn, 2000);
        write_chunk(&archive_dir, worker_id, 2, &next_lsn, 3000);

        // Time target inside the middle commit: stop after the 2000 commit.
        let lsn =
            resolve_target_lsn(&archive_dir, worker_id, 0, RestoreTarget::Time(2500)).unwrap();
        // Each batch is one frame (start lsn 0, 1, 2); the 2000 commit starts
        // at lsn 1, so its exclusive upper bound is 2.
        assert_eq!(lsn, 2);

        let lsn_end = resolve_target_lsn(&archive_dir, worker_id, 0, RestoreTarget::End).unwrap();
        // EOF high water: three batches at start lsn 0, 1, 2 -> 3.
        assert_eq!(lsn_end, 3);

        let lsn_lsn =
            resolve_target_lsn(&archive_dir, worker_id, 0, RestoreTarget::Lsn(7)).unwrap();
        assert_eq!(lsn_lsn, 7);

        mudu_sys::fs::sync::sync_remove_dir_all(&archive_dir).unwrap();
    }

    #[test]
    fn resolve_target_time_miss_is_an_error() {
        let archive_dir = mudu_sys::env_var::temp_dir()
            .join(format!("mudu_restore_miss_{}", mudu_utils::oid::gen_oid()))
            .to_string_lossy()
            .to_string();
        mudu_sys::fs::sync::create_dir_all(&archive_dir).unwrap();
        let next_lsn = AtomicU64::new(0);
        write_chunk(&archive_dir, 2u128, 0, &next_lsn, 1000);
        assert!(resolve_target_lsn(&archive_dir, 2u128, 0, RestoreTarget::Time(500)).is_err());
        mudu_sys::fs::sync::sync_remove_dir_all(&archive_dir).unwrap();
    }
}
