//! Per-worker replication maintenance: the periodic WAL/staging prefix GC
//! and the snapshot triggers that do not live on a serial apply point
//! (the stream primary's applied-watermark trigger).
//!
//! GC watermark (the first LSN to keep; whole chunks below it are
//! reclaimed):
//!
//! - raft leader: `min(chain resume, snapshot point + 1, min connected
//!   subscriber ack + 1)` — a connected subscriber holds the WAL back;
//! - raft follower: `min(chain resume, snapshot point + 1)`;
//! - stream primary: `min(applied low-water + 1, snapshot point + 1, min
//!   connected subscriber ack + 1)`;
//! - stream replica: `min(WAL end, snapshot point + 1)` (its WAL is the
//!   primary's stream copy; frames the snapshot covers are redundant).
//!
//! A subscriber whose resume point falls below the frontier reconnects
//! into a snapshot offer instead of a stream, which is what bounds the
//! WAL for slow/absent replicas.

/// The worker's replication index as `u32`.
fn worker_index_of(worker: &WorkerRuntime) -> RS<u32> {
    u32::try_from(worker.worker_index()).map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Parse,
            "worker index does not fit the replication protocol"
        )
    })
}
use crate::connected_subscriber_acks;
use crate::ctl::ReplModeCtl;
use crate::seg_hash::ZERO_SEG_HASH;
use crate::snapshot::{load_latest_manifest, take_worker_snapshot};
use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogLayout};
use ::worker::worker::WorkerRuntime;
use mudu::channel::ServerInstanceId;
use mudu::common::result::RS;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::tokio;
use std::time::Duration;
use tracing::{trace, warn};

/// Role-specific inputs of the maintenance loop.
pub enum MaintainDriver {
    /// Raft mode: the GC watermark reads the worker's committed WAL point
    /// from the data-plane service (resolved lazily; the service starts
    /// after the replication listener).
    Raft {
        /// The worker index this maintenance loop serves.
        worker_index: u32,
    },
    /// Stream mode: the primary snapshots at the applied low-water mark;
    /// the replica only GCs (its snapshots happen in the sink).
    Stream {
        /// The shared stream-mode control (role and applied watermarks).
        ctl: ReplModeCtl,
    },
}

/// Runs the maintenance loop of one worker until `stop_rx` fires.
pub async fn run_repl_maintenance(
    server_instance_id: ServerInstanceId,
    worker: WorkerRuntime,
    driver: MaintainDriver,
    snapshot_threshold_bytes: u64,
    gc_check_interval_secs: u64,
    mut stop_rx: StopRx,
) -> RS<()> {
    let worker_index = worker_index_of(&worker)?;
    let Some(log) = worker.worker_log()? else {
        warn!(
            worker_index,
            "repl maintenance: worker has no log, not starting"
        );
        return Ok(());
    };
    let layout = log.layout()?;
    let mut last_snapshot_lsn =
        load_latest_manifest(worker.storage().relation_path(), worker_index)?
            .map(|manifest| manifest.committed_lsn)
            .unwrap_or(0);
    let interval = Duration::from_secs(gc_check_interval_secs.max(1));
    loop {
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(interval) => {}
            _ = stop_rx.changed() => {
                return Ok(());
            }
        }
        if stop_rx.is_stopped() {
            return Ok(());
        }
        // Refresh the snapshot point every round: snapshots are taken by
        // the serial apply points (applier / sink), not by this loop.
        last_snapshot_lsn = load_latest_manifest(worker.storage().relation_path(), worker_index)?
            .map(|manifest| manifest.committed_lsn)
            .unwrap_or(last_snapshot_lsn);
        if let MaintainDriver::Stream { ctl } = &driver {
            maybe_primary_snapshot(
                &worker,
                worker_index,
                ctl,
                &mut last_snapshot_lsn,
                snapshot_threshold_bytes,
            )
            .await;
        }
        let gc_point = gc_watermark(
            server_instance_id,
            worker_index,
            &worker,
            &driver,
            last_snapshot_lsn,
        )
        .await?;
        gc_wal_prefix(server_instance_id, worker_index, &log, &layout, gc_point).await;
    }
}

/// Stream-primary snapshot trigger: the applied low-water mark is the only
/// point where every covered commit is fully applied.
async fn maybe_primary_snapshot(
    worker: &WorkerRuntime,
    worker_index: u32,
    ctl: &ReplModeCtl,
    last_snapshot_lsn: &mut u64,
    snapshot_threshold_bytes: u64,
) {
    if !ctl.is_primary() {
        return;
    }
    let water = ctl.applied_low_water();
    if water < *last_snapshot_lsn + snapshot_threshold_bytes {
        return;
    }
    match take_worker_snapshot(worker, worker_index, water, ZERO_SEG_HASH, 0, 0).await {
        Ok(_) => {
            *last_snapshot_lsn = water;
        }
        Err(err) => {
            warn!(
                worker_index,
                "stream primary snapshot failed (will retry): {err}"
            );
        }
    }
}

/// Reclaims the whole WAL chunks below `gc_point`; failures only log (the
/// next round retries).
async fn gc_wal_prefix(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    log: &ChunkedWorkerLogBackend,
    layout: &WorkerLogLayout,
    gc_point: u64,
) {
    if gc_point == 0 {
        return;
    }
    match layout.gc_prefix_chunks(log.fs().as_ref(), gc_point).await {
        Ok((freed, chunks)) => {
            if freed > 0 {
                crate::counters::note_wal_chunks_reclaimed(server_instance_id, chunks);
                trace!(worker_index, gc_point, freed, "worker WAL prefix reclaimed");
            }
        }
        Err(err) => {
            warn!(worker_index, "worker WAL GC failed: {err}");
        }
    }
}

/// The first LSN to keep for this worker (see the module doc).
async fn gc_watermark(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    worker: &WorkerRuntime,
    driver: &MaintainDriver,
    last_snapshot_lsn: u64,
) -> RS<u64> {
    if worker.storage().requires_recovery() || worker.storage().has_participant_history()? {
        return Ok(0);
    }
    let snapshot_keep = crate::snapshot::recovery_base_resume(worker)?
        .unwrap_or(0)
        .min(last_snapshot_lsn.saturating_add(1));
    let local_keep = match driver {
        MaintainDriver::Raft { worker_index } => {
            // The stream log (never prefix-GC'd yet) is the catch-up source
            // for lagging replicas, so the local WAL GC no longer gates on
            // connected subscribers. Until the data plane is up, keep
            // everything (watermark 0). The service registers before the
            // worker's data machine is installed (`run_data_worker_bootstrap`
            // waits for the service, then installs), so a registered but not
            // yet installed worker also keeps everything and retries next
            // round instead of failing the maintenance task.
            match crate::data_plane::data_plane_for(server_instance_id) {
                Some(service) => match service.wal_resume(*worker_index) {
                    Ok(resume) => {
                        // A live commit-driver read pins the floor: never
                        // reclaim below its in-flight segment position.
                        let floor = service.wal_read_floor(*worker_index);
                        if floor > 0 {
                            resume.min(floor)
                        } else {
                            resume
                        }
                    }
                    Err(err) if err.ec() == mudu::error::ErrorCode::InvalidState => 0,
                    Err(err) => return Err(err),
                },
                None => 0,
            }
        }
        MaintainDriver::Stream { ctl } => {
            if ctl.is_primary() {
                let applied_keep = ctl.applied_low_water().saturating_add(1);
                min_subscriber_keep(server_instance_id, worker_index)?
                    .map_or(applied_keep, |acks_keep| acks_keep.min(applied_keep))
            } else {
                // Replica: its WAL end is its applied point.
                let log = worker.worker_log()?;
                log.map(|log| log.next_lsn().as_u64()).unwrap_or(0)
            }
        }
    };
    Ok(local_keep.min(snapshot_keep))
}

/// `min connected subscriber ack + 1`, when any subscriber is connected.
fn min_subscriber_keep(server_instance_id: ServerInstanceId, worker_index: u32) -> RS<Option<u64>> {
    let acks = connected_subscriber_acks(server_instance_id, worker_index)?;
    Ok(acks.into_iter().min().map(|ack| ack.saturating_add(1)))
}
