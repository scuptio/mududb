//! Replication observability: the node-level status registry, the periodic
//! storage-usage cache, the operational counters, and the assembly of the
//! `GET /mudu/server/replication` response.
//!
//! Data sources are all live registries already maintained elsewhere (ack
//! watermarks, `RaftShare`, the stream admin entry, the applier chain
//! cells); the only new moving part is the storage-usage cache — directory
//! walks run in a 10-second background loop, never on the request path.

use crate::raft_service::RaftShare;
use crate::stream_admin::stream_admin_for_node;
use crate::{connected_subscriber_ack_pairs, repl_worker};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::catalog::repl_cfg::ReplMode;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// Interval of the storage-usage cache refresh.
const STORAGE_CACHE_INTERVAL: Duration = Duration::from_secs(10);

/// One member's view in the raft conf section.
#[derive(Clone, Debug)]
pub struct RaftStatusView {
    /// Current raft term.
    pub term: u64,
    /// Currently known leader, if any.
    pub leader_node_id: Option<u64>,
    /// Current commit index.
    pub commit_index: u64,
    /// Latest committed vector: `(worker_index, lsn, seg_hash hex)`.
    pub commit_vec: Vec<(u32, u64, String)>,
    /// Voting members of the committed conf.
    pub voters: Vec<u64>,
    /// Learner members of the committed conf.
    pub learners: Vec<u64>,
    /// Committed conf version `(term, version)`.
    pub conf_version_committed: (u64, u64),
    /// New (joint) conf version `(term, version)`; differs while a
    /// re-configuration is in progress.
    pub conf_version_new: (u64, u64),
}

/// The stream-mode section of the status.
#[derive(Clone, Debug)]
pub struct StreamStatusView {
    /// Current stream epoch.
    pub epoch: u64,
    /// The primary this node is or follows.
    pub primary_node_id: Option<u64>,
    /// Whether this node is the primary.
    pub is_primary: bool,
    /// Whether the primary waits for a replica's durable ack per commit.
    pub sync: bool,
    /// Synchronous replication wait timeout in milliseconds.
    pub sync_timeout_ms: u64,
}

/// One worker's replication watermarks.
#[derive(Clone, Debug)]
pub struct WorkerReplicationStatus {
    /// Worker index.
    pub worker_index: u32,
    /// Next LSN of the worker's WAL allocator (produced/applied high water).
    pub wal_next_lsn: u64,
    /// Raft committed chain point (resume LSN), raft mode only.
    pub chain_lsn: Option<u64>,
    /// Raft committed chain hash (hex), raft mode only.
    pub chain_seg_hash: Option<String>,
    /// Highest staged LSN (raft follower staging), when any.
    pub staging_max_lsn: Option<u64>,
    /// The worker's latest snapshot, when any.
    pub snapshot: Option<SnapshotStatusView>,
}

/// One worker's latest snapshot metadata.
#[derive(Clone, Debug)]
pub struct SnapshotStatusView {
    /// Committed LSN the snapshot covers.
    pub committed_lsn: u64,
    /// Wall-clock time the snapshot was taken (millis since the epoch).
    pub taken_at_ms: u64,
    /// Total size of the snapshot directory in bytes (from the periodic
    /// cache; 0 until the first refresh).
    pub dir_bytes: u64,
}

/// One subscriber's replication lag as the leader/primary sees it.
#[derive(Clone, Debug)]
pub struct SubscriberLagView {
    /// Subscriber's cluster node id.
    pub node_id: u64,
    /// The worker this subscription belongs to.
    pub worker_index: u32,
    /// Highest LSN the subscriber reported durably stored.
    pub stored_lsn: u64,
    /// The source's current high-water LSN on that worker.
    pub source_lsn: u64,
    /// `source_lsn - stored_lsn` (LSN/frame units).
    pub lag_lsn: u64,
}

/// Storage usage of the replication state (periodic cache).
#[derive(Clone, Debug, Default)]
pub struct ReplicationStorageView {
    /// Worker WAL chunk bytes (all workers).
    pub wal_bytes: u64,
    /// Staging store bytes (all workers).
    pub staging_bytes: u64,
    /// Raft log bytes.
    pub raft_log_bytes: u64,
    /// Snapshot directory bytes (all workers).
    pub snapshot_bytes: u64,
    /// Per-worker snapshot directory bytes (index = worker index).
    pub snapshot_worker_bytes: Vec<u64>,
    /// When the cache was last refreshed (millis since the epoch).
    pub refreshed_at_ms: u64,
}

/// Operational counters of the replication layer (this server's lifetime).
#[derive(Clone, Debug, Default)]
pub struct ReplicationCounters {
    /// Synchronous stream commits that failed on the replica-ack timeout.
    pub sync_write_timeouts: u64,
    /// Worker snapshots taken locally.
    pub snapshots_taken: u64,
    /// Worker WAL chunks reclaimed by the prefix GC.
    pub wal_chunks_reclaimed: u64,
    /// Staging chunks reclaimed by the prefix GC.
    pub staging_chunks_reclaimed: u64,
    /// Snapshot offers served to lagging subscribers.
    pub snapshot_offers_served: u64,
    /// Snapshot restores completed on this node.
    pub snapshot_restores_completed: u64,
}

/// The assembled replication status of one node.
#[derive(Clone, Debug)]
pub struct ServerReplicationStatus {
    /// `"raft"` | `"stream"`.
    pub mode: String,
    /// This node's cluster id.
    pub node_id: u64,
    /// Raft role or `"primary"` / `"replica"` (stream).
    pub role: String,
    /// Whether client writes are accepted on this node.
    pub serving: bool,
    /// Raft section (raft mode only).
    pub raft: Option<RaftStatusView>,
    /// Stream section (stream mode only).
    pub stream: Option<StreamStatusView>,
    /// Per-worker replication watermarks.
    pub workers: Vec<WorkerReplicationStatus>,
    /// Subscriber lags as this node sees them (leader/primary view).
    pub subscribers: Vec<SubscriberLagView>,
    /// Storage usage (periodic cache).
    pub storage: ReplicationStorageView,
    /// Operational counters.
    pub counters: ReplicationCounters,
}

struct StatusEntry {
    mode: ReplMode,
    node_id: u64,
    server_instance_id: ServerInstanceId,
    data_dir: PathBuf,
    log_dir: PathBuf,
    worker_count: usize,
}

/// The status slot of one node: the static entry plus the cached storage
/// sizes the background refresh task publishes.
pub struct StatusCacheEntry {
    entry: StatusEntry,
    storage: ReplicationStorageView,
}

fn status_registry() -> &'static SMutex<HashMap<u64, Arc<SMutex<StatusCacheEntry>>>> {
    static REGISTRY: OnceLock<SMutex<HashMap<u64, Arc<SMutex<StatusCacheEntry>>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers the node's replication status entry (server startup,
/// replication configured), and returns the shared cache slot the
/// background refresh task fills.
pub fn register_replication_status(
    mode: ReplMode,
    node_id: u64,
    server_instance_id: ServerInstanceId,
    data_dir: PathBuf,
    log_dir: PathBuf,
    worker_count: usize,
) -> RS<Arc<SMutex<StatusCacheEntry>>> {
    let slot = Arc::new(SMutex::new(StatusCacheEntry {
        entry: StatusEntry {
            mode,
            node_id,
            server_instance_id,
            data_dir,
            log_dir,
            worker_count,
        },
        storage: ReplicationStorageView::default(),
    }));
    status_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl status registry lock poisoned"))?
        .insert(node_id, slot.clone());
    Ok(slot)
}

/// Removes the status registration (server shutdown).
pub fn unregister_replication_status(node_id: u64) -> RS<()> {
    status_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl status registry lock poisoned"))?
        .remove(&node_id);
    Ok(())
}

fn status_entry(node_id: u64) -> Option<Arc<SMutex<StatusCacheEntry>>> {
    status_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&node_id).cloned())
}

fn counter_slot(server_instance_id: ServerInstanceId) -> Arc<ReplicationCountersAtomic> {
    static SLOTS: OnceLock<SMutex<HashMap<ServerInstanceId, Arc<ReplicationCountersAtomic>>>> =
        OnceLock::new();
    let slots = SLOTS.get_or_init(|| SMutex::new(HashMap::new()));
    match slots.lock() {
        Ok(mut map) => map.entry(server_instance_id).or_default().clone(),
        Err(_) => Arc::new(ReplicationCountersAtomic::default()),
    }
}

#[derive(Default)]
struct ReplicationCountersAtomic {
    sync_write_timeouts: AtomicU64,
    snapshots_taken: AtomicU64,
    wal_chunks_reclaimed: AtomicU64,
    staging_chunks_reclaimed: AtomicU64,
}

impl ReplicationCountersAtomic {
    fn snapshot(&self) -> ReplicationCounters {
        ReplicationCounters {
            sync_write_timeouts: self.sync_write_timeouts.load(Ordering::Relaxed),
            snapshots_taken: self.snapshots_taken.load(Ordering::Relaxed),
            wal_chunks_reclaimed: self.wal_chunks_reclaimed.load(Ordering::Relaxed),
            staging_chunks_reclaimed: self.staging_chunks_reclaimed.load(Ordering::Relaxed),
            snapshot_offers_served: crate::test_hook_snapshot_offers_served(),
            snapshot_restores_completed: crate::test_hook_snapshot_restores_completed(),
        }
    }
}

/// Counter bump: a synchronous stream commit hit the replica-ack timeout.
pub(crate) fn note_sync_write_timeout(server_instance_id: ServerInstanceId) {
    counter_slot(server_instance_id)
        .sync_write_timeouts
        .fetch_add(1, Ordering::Relaxed);
}

/// Counter bump: a worker snapshot was taken on this server.
pub(crate) fn note_snapshot_taken(server_instance_id: ServerInstanceId) {
    counter_slot(server_instance_id)
        .snapshots_taken
        .fetch_add(1, Ordering::Relaxed);
}

/// Counter bump: worker WAL chunks were reclaimed by the prefix GC.
pub(crate) fn note_wal_chunks_reclaimed(server_instance_id: ServerInstanceId, chunks: u64) {
    counter_slot(server_instance_id)
        .wal_chunks_reclaimed
        .fetch_add(chunks, Ordering::Relaxed);
}

/// Counter bump: staging chunks were reclaimed by the prefix GC.
pub(crate) fn note_staging_chunks_reclaimed(server_instance_id: ServerInstanceId, chunks: u64) {
    counter_slot(server_instance_id)
        .staging_chunks_reclaimed
        .fetch_add(chunks, Ordering::Relaxed);
}

/// Whether a replication status entry exists for the node (the
/// single-node answer is assembled by the caller).
pub fn replication_status_registered(node_id: u64) -> bool {
    status_entry(node_id).is_some()
}

/// Runs the periodic storage-usage refresh loop for one node until
/// `stop_rx` fires. Spawned per server when replication is configured.
pub async fn run_replication_status_cache(
    slot: Arc<SMutex<StatusCacheEntry>>,
    mut stop_rx: mudu_sys::sync::async_::stop_flag::StopRx,
) -> RS<()> {
    refresh_storage_cache(&slot)?;
    loop {
        mudu_sys::tokio::select! {
            _ = mudu_sys::task::async_::sleep(STORAGE_CACHE_INTERVAL) => {}
            _ = stop_rx.changed() => {
                return Ok(());
            }
        }
        if stop_rx.is_stopped() {
            return Ok(());
        }
        refresh_storage_cache(&slot)?;
    }
}

/// Walks the storage directories once and publishes the sizes.
fn refresh_storage_cache(slot: &Arc<SMutex<StatusCacheEntry>>) -> RS<()> {
    let (data_dir, log_dir, worker_count) = {
        let guard = slot
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl status cache lock poisoned"))?;
        (
            guard.entry.data_dir.clone(),
            guard.entry.log_dir.clone(),
            guard.entry.worker_count,
        )
    };
    let mut wal_bytes = 0u64;
    if let Ok(entries) = mudu_sys::fs::sync::read_dir(&data_dir) {
        for path in entries {
            let keep = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".xl"));
            if keep {
                wal_bytes += mudu_sys::fs::sync::metadata(&path)
                    .map(|meta| meta.len())
                    .unwrap_or(0);
            }
        }
    }
    let staging_bytes = dir_size(&log_dir.join("repl_staging"));
    let raft_log_bytes = dir_size(&log_dir.join("raft_log"));
    let snapshot_root = data_dir.join("snapshot");
    let snapshot_bytes = dir_size(&snapshot_root);
    let mut snapshot_worker_bytes = Vec::with_capacity(worker_count);
    for worker_index in 0..worker_count {
        snapshot_worker_bytes.push(dir_size(
            &snapshot_root.join(format!("worker_{worker_index}")),
        ));
    }
    let refreshed_at_ms = mudu_sys::time::system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut guard = slot
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl status cache lock poisoned"))?;
    guard.storage = ReplicationStorageView {
        wal_bytes,
        staging_bytes,
        raft_log_bytes,
        snapshot_bytes,
        snapshot_worker_bytes,
        refreshed_at_ms,
    };
    Ok(())
}

/// Total size of a directory tree in bytes (missing = 0).
fn dir_size(root: &Path) -> u64 {
    if !mudu_sys::fs::sync::sync_path_exists(root) {
        return 0;
    }
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = mudu_sys::fs::sync::read_dir(&dir) {
            for path in entries {
                if path.is_dir() {
                    stack.push(path);
                } else {
                    total += mudu_sys::fs::sync::metadata(&path)
                        .map(|meta| meta.len())
                        .unwrap_or(0);
                }
            }
        }
    }
    total
}

/// Assembles the replication status of the given node. Returns `Ok(None)`
/// when the node runs no replication (the HTTP layer answers `mode=none`).
pub fn collect_replication_status(node_id: u64) -> RS<Option<ServerReplicationStatus>> {
    let Some(slot) = status_entry(node_id) else {
        return Ok(None);
    };
    let (entry, storage) = {
        let guard = slot
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl status cache lock poisoned"))?;
        (
            (
                guard.entry.mode,
                guard.entry.node_id,
                guard.entry.server_instance_id,
                guard.entry.data_dir.clone(),
                guard.entry.worker_count,
            ),
            guard.storage.clone(),
        )
    };
    let (mode, node_id, server_instance_id, data_dir, worker_count) = entry;
    let counters = counter_slot(server_instance_id).snapshot();

    let Some(mode_view) = collect_mode_view(mode, node_id)? else {
        return Ok(None);
    };

    let mut workers = Vec::with_capacity(worker_count);
    let mut subscribers = Vec::new();
    for worker_index in 0..worker_count as u32 {
        let Some((worker_view, mut lag_views)) = collect_worker_view(
            server_instance_id,
            &data_dir,
            &storage,
            worker_index,
            mode == ReplMode::Raft,
        )?
        else {
            continue;
        };
        workers.push(worker_view);
        subscribers.append(&mut lag_views);
    }
    Ok(Some(ServerReplicationStatus {
        mode: mode_view.mode_text,
        node_id,
        role: mode_view.role,
        serving: mode_view.serving,
        raft: mode_view.raft,
        stream: mode_view.stream,
        workers,
        subscribers,
        storage,
        counters,
    }))
}

/// The mode-specific section of the status (raft or stream).
struct ModeStatusView {
    mode_text: String,
    role: String,
    serving: bool,
    raft: Option<RaftStatusView>,
    stream: Option<StreamStatusView>,
}

/// Collects the mode-specific section; `None` when the node's mode service
/// (raft share / stream admin) is not running.
fn collect_mode_view(mode: ReplMode, node_id: u64) -> RS<Option<ModeStatusView>> {
    match mode {
        ReplMode::Raft => {
            let Some(share) = crate::raft_share_for_node(node_id) else {
                return Ok(None);
            };
            let (role, serving, raft) = collect_raft_view(&share);
            Ok(Some(ModeStatusView {
                mode_text: "raft".to_string(),
                role,
                serving,
                raft: Some(raft),
                stream: None,
            }))
        }
        ReplMode::Stream => {
            let Some(admin) = stream_admin_for_node(node_id) else {
                return Ok(None);
            };
            let (role, serving, stream) = collect_stream_view(&admin, node_id);
            Ok(Some(ModeStatusView {
                mode_text: "stream".to_string(),
                role,
                serving,
                raft: None,
                stream: Some(stream),
            }))
        }
    }
}

/// Collects the raft section: status snapshot, committed vector and conf.
fn collect_raft_view(share: &RaftShare) -> (String, bool, RaftStatusView) {
    let status = share.status_snapshot();
    let voters = share.voter_ids();
    let learners: Vec<u64> = share
        .peers()
        .iter()
        .map(|peer| peer.node_id())
        .filter(|id| !voters.contains(id))
        .collect();
    let commit_vec: Vec<(u32, u64, String)> = share
        .committed_vector()
        .into_iter()
        .map(|(worker_index, (lsn, hash))| {
            (worker_index, lsn, crate::seg_hash::seg_hash_hex(&hash))
        })
        .collect();
    let role = format!("{:?}", status.role);
    (
        role,
        share.is_serving(),
        RaftStatusView {
            term: status.term,
            leader_node_id: status.leader_id,
            commit_index: status.commit_index,
            commit_vec,
            voters,
            learners,
            conf_version_committed: (
                status.conf_version_committed.term,
                status.conf_version_committed.version,
            ),
            conf_version_new: (
                status.conf_version_new.term,
                status.conf_version_new.version,
            ),
        },
    )
}

/// Collects the stream section: epoch, primary and the sync policy.
fn collect_stream_view(
    admin: &crate::stream_admin::StreamAdminEntry,
    node_id: u64,
) -> (String, bool, StreamStatusView) {
    let ctl = &admin.ctl;
    let is_primary = ctl.is_primary();
    (
        if is_primary {
            "primary".to_string()
        } else {
            "replica".to_string()
        },
        is_primary,
        StreamStatusView {
            epoch: ctl.epoch(),
            primary_node_id: if is_primary {
                Some(node_id)
            } else {
                ctl.primary_node_id()
            },
            is_primary,
            sync: ctl.sync(),
            sync_timeout_ms: ctl.sync_timeout().as_millis() as u64,
        },
    )
}

/// Collects one worker's replication watermarks plus its subscriber lag
/// rows as this node sees them; `None` when the worker has no repl handle.
fn collect_worker_view(
    server_instance_id: ServerInstanceId,
    data_dir: &Path,
    storage: &ReplicationStorageView,
    worker_index: u32,
    is_raft: bool,
) -> RS<Option<(WorkerReplicationStatus, Vec<SubscriberLagView>)>> {
    let handle = match repl_worker(server_instance_id, worker_index) {
        Ok(handle) => handle,
        Err(_) => return Ok(None),
    };
    let wal_next_lsn = handle
        .worker()
        .worker_log()?
        .map(|log| log.next_lsn().as_u64())
        .unwrap_or(0);
    let chain = handle.state().chain()?;
    let staging_max = match handle.state().staging()? {
        Some(staging) => staging.max_lsn()?.map(|lsn| lsn.as_u64()),
        None => None,
    };
    let snapshot =
        crate::snapshot::load_latest_manifest(data_dir.to_string_lossy().as_ref(), worker_index)?
            .map(|manifest| SnapshotStatusView {
                committed_lsn: manifest.committed_lsn,
                taken_at_ms: manifest.taken_at_ms,
                dir_bytes: storage
                    .snapshot_worker_bytes
                    .get(worker_index as usize)
                    .copied()
                    .unwrap_or(0),
            });
    let worker_view = WorkerReplicationStatus {
        worker_index,
        wal_next_lsn,
        chain_lsn: is_raft.then_some(chain.0),
        chain_seg_hash: is_raft.then(|| crate::seg_hash::seg_hash_hex(&chain.1)),
        staging_max_lsn: staging_max,
        snapshot,
    };
    let source_lsn = wal_next_lsn.saturating_sub(1);
    let mut lag_views = Vec::new();
    for (subscriber, stored) in connected_subscriber_ack_pairs(server_instance_id, worker_index)? {
        lag_views.push(SubscriberLagView {
            node_id: subscriber,
            worker_index,
            stored_lsn: stored,
            source_lsn,
            lag_lsn: source_lsn.saturating_sub(stored),
        });
    }
    Ok(Some((worker_view, lag_views)))
}
