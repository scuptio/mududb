//! Primary-replica streaming replication with Raft fault tolerance.
//!
//! Every cluster node runs a node-level replication listener ([`source`])
//! that streams worker WAL frames to subscribed followers and multiplexes
//! raft traffic; each follower worker runs a sink ([`sink`]) that stages
//! streamed frames and an applier ([`applier`]) that promotes
//! raft-committed frames into the worker WAL; the raft group itself runs in
//! [`raft_service`]; the leader's commit path is quorum-gated by the
//! per-worker [`commit_sequencer`]; leader-election healing lives in
//! [`heal`].

#![warn(missing_docs)]

/// Follower-side replication sink, one task per worker.
pub mod sink;
/// Node-level replication listener serving WAL streams and raft traffic.
pub mod source;

/// Promote applier: moves raft-committed frames from staging into the
/// worker WAL.
pub mod applier;
/// Per-worker commit sequencer on the raft leader.
pub mod commit_sequencer;
/// Replication-mode (non-raft) control for the write gate and the primary's
/// synchronous commit path.
pub mod ctl;
/// Leader-elect heal: converges every worker to the last raft-committed
/// vector before client writes are ungated.
pub mod heal;
/// Per-worker replication maintenance: periodic WAL/staging prefix GC and
/// snapshot triggers.
pub mod maintain;
/// Raft group lifecycle and the MSFM transport for raft traffic.
pub mod raft_service;
/// Segment hash chain for raft-committed WAL segments.
pub mod seg_hash;
/// Per-worker business snapshots and the snapshot transfer layout.
pub mod snapshot;
/// Follower-side staging store for streamed WAL frames.
pub mod staging;
/// Replication observability: status registry and operational counters.
pub mod status;
/// Stream-mode runtime role administration (promote/follow orchestration).
pub mod stream_admin;
/// Monotonic frame cursor over a worker log's chunk files.
pub mod wal_cursor;

pub use applier::run_repl_applier;
pub use commit_sequencer::{new_commit_sequencer, run_commit_sequencer};
pub use raft_service::{start_raft_service, RaftShare};
pub use sink::{run_repl_sink, SinkDriver};
pub use source::spawn_repl_source;
pub use staging::StagingStore;

use crate::seg_hash::SegHash;
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::catalog::frame::try_decode_next_frame;
use ::wal::lsn::LSN;
use ::worker::worker::WorkerRuntime;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::Frame;
use mudu_raft::RaftHandle;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::sync::{SMutex, SRwLock};
use mudu_sys::tokio;
use mudu_sys::tokio::io::{AsyncRead, AsyncReadExt};
use mudu_sys::tokio::sync::mpsc::UnboundedSender;
use mudu_sys::tokio::sync::oneshot;
use mudu_sys::tokio::sync::Notify;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Messages for a worker's promote applier task.
pub enum ApplierMsg {
    /// Promote the worker's chain to this raft-committed point. `respond`
    /// is set by the leader-elect heal, which waits for the promotion.
    /// `raft_term` / `raft_index` locate the CommitVec entry in the raft
    /// log (the snapshot manifest's install frontier).
    Promote {
        /// The raft-committed LSN to promote the chain to.
        lsn: u64,
        /// Segment hash at `lsn`.
        hash: SegHash,
        /// Raft term of the promote point's CommitVec entry.
        raft_term: u64,
        /// Raft index of the promote point's CommitVec entry.
        raft_index: u64,
        /// Completion channel, set by the leader-elect heal.
        respond: Option<oneshot::Sender<RS<()>>>,
    },
    /// Committed frames fetched from a voter by the heal flow; the applier
    /// consumes them before falling back to waiting on the stream.
    HealFrames {
        /// The fetched committed frames, in LSN order.
        frames: Vec<Vec<u8>>,
    },
    /// Drop every staged frame beyond the current committed point
    /// (leader-elect heal discards the speculative staged tail).
    DiscardStaging {
        /// Completion channel.
        respond: Option<oneshot::Sender<RS<()>>>,
    },
    /// Install a fetched snapshot: replace storage content, GC the WAL
    /// below the snapshot point and re-anchor the chain at it. The sink
    /// fetched and verified the files; the applier mutates serially.
    InstallSnapshot {
        /// The verified snapshot manifest.
        manifest: mudu_contract::protocol::ReplSnapshotManifest,
        /// Directory holding the fetched snapshot files.
        snapshot_dir: std::path::PathBuf,
        /// Completion channel.
        respond: oneshot::Sender<RS<()>>,
    },
}

/// Shared per-worker replication state, attached at worker registration and
/// filled in as the worker's replication tasks start.
pub struct ReplWorkerState {
    /// The worker's committed chain state `(lsn, seg_hash)`, owned by the
    /// applier (single writer) and read by the sink (subscribe point), the
    /// sequencer (chain continuation) and the heal flow.
    chain: SRwLock<(u64, SegHash)>,
    /// Staging store, attached when the sink starts.
    staging: SRwLock<Option<Arc<StagingStore>>>,
    /// Applier channel, attached when the applier starts.
    applier_tx: SRwLock<Option<UnboundedSender<ApplierMsg>>>,
    /// Wakes the commit sequencer when a new ack watermark arrives.
    ack_notify: Notify,
    /// Wakes the applier when the sink stages new frames.
    staging_notify: Notify,
}

impl ReplWorkerState {
    fn new() -> Self {
        Self {
            chain: SRwLock::new((0, seg_hash::ZERO_SEG_HASH)),
            staging: SRwLock::new(None),
            applier_tx: SRwLock::new(None),
            ack_notify: Notify::new(),
            staging_notify: Notify::new(),
        }
    }

    /// The current committed chain point.
    pub fn chain(&self) -> RS<(u64, SegHash)> {
        Ok(*self
            .chain
            .read()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl chain lock poisoned"))?)
    }

    /// Advances the committed chain point (applier only).
    pub fn set_chain(&self, lsn: u64, hash: SegHash) -> RS<()> {
        *self
            .chain
            .write()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl chain lock poisoned"))? =
            (lsn, hash);
        Ok(())
    }

    /// The staging store, when the sink has attached it.
    pub fn staging(&self) -> RS<Option<Arc<StagingStore>>> {
        Ok(self
            .staging
            .read()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl staging lock poisoned"))?
            .clone())
    }

    /// Attaches the staging store (sink startup).
    pub fn set_staging(&self, staging: Arc<StagingStore>) -> RS<()> {
        *self
            .staging
            .write()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl staging lock poisoned"))? =
            Some(staging);
        Ok(())
    }

    /// The applier channel, when the applier has attached it.
    pub fn applier_tx(&self) -> RS<Option<UnboundedSender<ApplierMsg>>> {
        Ok(self
            .applier_tx
            .read()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl applier lock poisoned"))?
            .clone())
    }

    /// Attaches the applier channel (applier startup).
    pub fn set_applier_tx(&self, tx: UnboundedSender<ApplierMsg>) -> RS<()> {
        *self
            .applier_tx
            .write()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl applier lock poisoned"))? =
            Some(tx);
        Ok(())
    }

    /// Wakes a sequencer waiting on ack watermarks.
    pub fn notify_ack(&self) {
        self.ack_notify.notify_one();
    }

    /// Waits for the next ack watermark arrival.
    pub async fn ack_notified(&self) {
        self.ack_notify.notified().await;
    }

    /// Wakes the applier waiting for newly staged frames (sink append).
    pub fn notify_staging(&self) {
        self.staging_notify.notify_one();
    }

    /// Waits for the next staging-append wake.
    pub async fn staging_notified(&self) {
        self.staging_notify.notified().await;
    }
}

/// Handle to a worker registered for replication.
pub struct ReplWorkerHandle {
    worker: Arc<WorkerRuntime>,
    state: Arc<ReplWorkerState>,
}

impl ReplWorkerHandle {
    /// Creates a handle wrapping the given worker.
    pub fn new(worker: Arc<WorkerRuntime>) -> Self {
        Self {
            worker,
            state: Arc::new(ReplWorkerState::new()),
        }
    }

    /// The registered worker.
    pub fn worker(&self) -> &Arc<WorkerRuntime> {
        &self.worker
    }

    /// The worker's shared replication state.
    pub fn state(&self) -> &Arc<ReplWorkerState> {
        &self.state
    }
}

fn repl_worker_registry() -> &'static SMutex<HashMap<(ServerInstanceId, u32), ReplWorkerHandle>> {
    static REGISTRY: OnceLock<SMutex<HashMap<(ServerInstanceId, u32), ReplWorkerHandle>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers a worker for replication under its worker index. Mirrors the
/// message-bus registry lifecycle: registration happens when the worker
/// starts and unregistration after its serving loop exits.
pub fn register_repl_worker(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    handle: ReplWorkerHandle,
) -> RS<()> {
    let mut registry = repl_worker_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl worker registry lock poisoned"))?;
    registry.insert((server_instance_id, worker_index), handle);
    Ok(())
}

/// Removes a worker from the replication registry; unknown entries are
/// ignored so shutdown stays idempotent.
pub fn unregister_repl_worker(server_instance_id: ServerInstanceId, worker_index: u32) -> RS<()> {
    let mut registry = repl_worker_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl worker registry lock poisoned"))?;
    registry.remove(&(server_instance_id, worker_index));
    Ok(())
}

/// Resolves the replication handle of the worker with the given index.
pub fn repl_worker(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
) -> RS<ReplWorkerHandle> {
    let registry = repl_worker_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl worker registry lock poisoned"))?;
    registry
        .get(&(server_instance_id, worker_index))
        .map(|handle| ReplWorkerHandle {
            worker: handle.worker.clone(),
            state: handle.state.clone(),
        })
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!(
                    "repl worker for server {} worker index {} is not registered",
                    server_instance_id, worker_index
                )
            )
        })
}

/// Per-worker ack watermarks: subscriber node id -> highest stored LSN it
/// reported, keyed by `(server_instance_id, worker_index)`.
type ReplAckWatermarks = HashMap<(ServerInstanceId, u32), HashMap<u64, u64>>;

fn repl_ack_state() -> &'static SMutex<ReplAckWatermarks> {
    static STATE: OnceLock<SMutex<ReplAckWatermarks>> = OnceLock::new();
    STATE.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Records the durable stored LSN reported by a replica (`src_node_id`) for
/// the given worker, and wakes that worker's commit sequencer.
pub fn record_repl_ack(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    src_node_id: u64,
    stored_lsn: u64,
) -> RS<()> {
    let mut state = repl_ack_state()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ack state lock poisoned"))?;
    let watermarks = state.entry((server_instance_id, worker_index)).or_default();
    let entry = watermarks.entry(src_node_id).or_insert(0);
    *entry = (*entry).max(stored_lsn);
    drop(state);
    tracing::trace!(worker_index, src_node_id, stored_lsn, "repl ack recorded");
    if let Ok(handle) = repl_worker(server_instance_id, worker_index) {
        handle.state().notify_ack();
    }
    Ok(())
}

/// Returns the recorded `(src_node_id, stored_lsn)` ack watermarks of the
/// given worker, ordered by node id.
pub fn ack_watermarks(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
) -> RS<Vec<(u64, u64)>> {
    let state = repl_ack_state()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ack state lock poisoned"))?;
    let mut watermarks: Vec<(u64, u64)> = state
        .get(&(server_instance_id, worker_index))
        .map(|entries| entries.iter().map(|(node, lsn)| (*node, *lsn)).collect())
        .unwrap_or_default();
    watermarks.sort_by_key(|(node, _)| *node);
    Ok(watermarks)
}

/// Drops every ack watermark of the server. Called by the leader-elect
/// heal: after the healed WAL truncation, new segments reuse LSNs and acks
/// recorded under the previous leadership must not open the majority gate.
pub fn clear_ack_watermarks(server_instance_id: ServerInstanceId) -> RS<()> {
    let mut state = repl_ack_state()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ack state lock poisoned"))?;
    state.retain(|(instance, _), _| *instance != server_instance_id);
    Ok(())
}

/// Currently connected subscribers of a worker's replication stream
/// (source side), used by the WAL GC: only connected subscribers hold the
/// leader's WAL back — a disconnected replica rejoins through the snapshot
/// offer when its resume point fell below the GC frontier.
type ReplSubscribers = HashMap<(ServerInstanceId, u32), HashMap<u64, ()>>;

fn repl_subscriber_state() -> &'static SMutex<ReplSubscribers> {
    static STATE: OnceLock<SMutex<ReplSubscribers>> = OnceLock::new();
    STATE.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Marks subscriber `src_node_id` connected to this server's worker stream.
pub fn register_repl_subscriber(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    src_node_id: u64,
) -> RS<()> {
    let mut state = repl_subscriber_state()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl subscriber state lock poisoned"))?;
    state
        .entry((server_instance_id, worker_index))
        .or_default()
        .insert(src_node_id, ());
    Ok(())
}

/// Marks the subscriber disconnected (subscription task exit).
pub fn unregister_repl_subscriber(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    src_node_id: u64,
) -> RS<()> {
    let mut state = repl_subscriber_state()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl subscriber state lock poisoned"))?;
    if let Some(subscribers) = state.get_mut(&(server_instance_id, worker_index)) {
        subscribers.remove(&src_node_id);
    }
    Ok(())
}

/// Ack watermarks of the currently connected subscribers of a worker
/// (the leader-side GC input).
pub fn connected_subscriber_acks(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
) -> RS<Vec<u64>> {
    Ok(
        connected_subscriber_ack_pairs(server_instance_id, worker_index)?
            .into_iter()
            .map(|(_, stored)| stored)
            .collect(),
    )
}

/// `(subscriber node id, stored lsn)` of the currently connected
/// subscribers of a worker (leader/primary observability view).
pub fn connected_subscriber_ack_pairs(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
) -> RS<Vec<(u64, u64)>> {
    let subscribers = repl_subscriber_state()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl subscriber state lock poisoned"))?
        .get(&(server_instance_id, worker_index))
        .map(|entries| entries.keys().copied().collect::<Vec<u64>>())
        .unwrap_or_default();
    let acks = repl_ack_state()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ack state lock poisoned"))?;
    let watermarks = acks.get(&(server_instance_id, worker_index));
    Ok(subscribers
        .into_iter()
        .filter_map(|node| {
            watermarks
                .and_then(|w| w.get(&node).copied())
                .map(|stored| (node, stored))
        })
        .collect())
}

/// Test-hook counters for the snapshot offer/restore paths.
#[derive(Default)]
struct SnapshotTestHooks {
    offers_served: std::sync::atomic::AtomicU64,
    restores_completed: std::sync::atomic::AtomicU64,
}

fn snapshot_test_hooks() -> &'static SnapshotTestHooks {
    static HOOKS: OnceLock<SnapshotTestHooks> = OnceLock::new();
    HOOKS.get_or_init(SnapshotTestHooks::default)
}

/// Test hook: snapshot offers served by this process.
pub fn test_hook_snapshot_offers_served() -> u64 {
    snapshot_test_hooks()
        .offers_served
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// Test hook: snapshot restores completed by this process.
pub fn test_hook_snapshot_restores_completed() -> u64 {
    snapshot_test_hooks()
        .restores_completed
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// Increments the snapshot-offer counter (source side).
pub fn note_snapshot_offer_served() {
    snapshot_test_hooks()
        .offers_served
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Increments the snapshot-restore counter (sink side).
pub fn note_snapshot_restore_completed() {
    snapshot_test_hooks()
        .restores_completed
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Registry of per-server raft groups: the raft handle and its shared
/// state, keyed by server instance id.
type RaftRegistry = HashMap<ServerInstanceId, (Arc<RaftHandle>, Arc<RaftShare>)>;

fn raft_registry() -> &'static SMutex<RaftRegistry> {
    static REGISTRY: OnceLock<SMutex<RaftRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers the server's raft handle and share (raft service startup).
pub fn register_raft(
    server_instance_id: ServerInstanceId,
    raft: Arc<RaftHandle>,
    share: Arc<RaftShare>,
) -> RS<()> {
    raft_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft registry lock poisoned"))?
        .insert(server_instance_id, (raft, share));
    Ok(())
}

/// Removes the server's raft registration (raft service shutdown).
pub fn unregister_raft(server_instance_id: ServerInstanceId) -> RS<()> {
    raft_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft registry lock poisoned"))?
        .remove(&server_instance_id);
    Ok(())
}

/// The raft handle of the given server, when the raft service runs.
pub fn raft_for(server_instance_id: ServerInstanceId) -> RS<Option<Arc<RaftHandle>>> {
    Ok(raft_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft registry lock poisoned"))?
        .get(&server_instance_id)
        .map(|(raft, _)| raft.clone()))
}

fn raft_shares_by_node() -> &'static SMutex<HashMap<u64, Arc<RaftShare>>> {
    static SHARES: OnceLock<SMutex<HashMap<u64, Arc<RaftShare>>>> = OnceLock::new();
    SHARES.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers a share under its cluster node id (raft service startup).
pub fn raft_shares_by_node_register(share: Arc<RaftShare>) -> RS<()> {
    raft_shares_by_node()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft share registry lock poisoned"))?
        .insert(share.node_id(), share);
    Ok(())
}

/// Removes the share of the given node id (raft service shutdown).
pub fn raft_shares_by_node_unregister(node_id: u64) -> RS<()> {
    raft_shares_by_node()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft share registry lock poisoned"))?
        .remove(&node_id);
    Ok(())
}

/// The raft share of the server running with the given cluster node id
/// (HTTP topology endpoint and tests).
pub fn raft_share_for_node(node_id: u64) -> Option<Arc<RaftShare>> {
    raft_shares_by_node()
        .lock()
        .ok()
        .and_then(|shares| shares.get(&node_id).cloned())
}

fn raft_handles_by_node() -> &'static SMutex<HashMap<u64, Arc<RaftHandle>>> {
    static HANDLES: OnceLock<SMutex<HashMap<u64, Arc<RaftHandle>>>> = OnceLock::new();
    HANDLES.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers the raft handle under its cluster node id (raft service
/// startup), so the membership endpoint can reach the group without the
/// server instance id.
pub fn raft_handle_by_node_register(node_id: u64, raft: Arc<RaftHandle>) -> RS<()> {
    raft_handles_by_node()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft handle registry lock poisoned"))?
        .insert(node_id, raft);
    Ok(())
}

/// Removes the handle of the given node id (raft service shutdown).
pub fn raft_handle_by_node_unregister(node_id: u64) -> RS<()> {
    raft_handles_by_node()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft handle registry lock poisoned"))?
        .remove(&node_id);
    Ok(())
}

/// The raft handle of the server running with the given cluster node id
/// (HTTP membership endpoint).
pub fn raft_handle_for_node(node_id: u64) -> Option<Arc<RaftHandle>> {
    raft_handles_by_node()
        .lock()
        .ok()
        .and_then(|handles| handles.get(&node_id).cloned())
}

/// Kernel-side member address book: cluster node id -> replication
/// endpoint (`host:port`). Seeded from the static peer list at startup and
/// kept in sync with committed membership changes: the raft conf carries
/// only node ids, so the replication addresses travel out of band (the
/// membership request) and are reconciled here.
fn repl_member_addrs() -> &'static SMutex<HashMap<u64, (String, u16)>> {
    static ADDRS: OnceLock<SMutex<HashMap<u64, (String, u16)>>> = OnceLock::new();
    ADDRS.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers (or replaces) the replication address of a cluster member.
pub fn repl_member_addr_register(node_id: u64, host: String, port: u16) -> RS<()> {
    repl_member_addrs()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl member address lock poisoned"))?
        .insert(node_id, (host, port));
    Ok(())
}

/// Drops the address of a removed cluster member.
pub fn repl_member_addr_unregister(node_id: u64) -> RS<()> {
    repl_member_addrs()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl member address lock poisoned"))?
        .remove(&node_id);
    Ok(())
}

/// The replication address of a cluster member, when known.
pub fn repl_member_addr(node_id: u64) -> Option<(String, u16)> {
    repl_member_addrs()
        .lock()
        .ok()
        .and_then(|addrs| addrs.get(&node_id).cloned())
}

/// Test hook: pauses (or resumes) the replication sinks of the server
/// running with the given cluster node id, simulating an isolated follower.
pub fn test_hook_set_sink_paused(node_id: u64, paused: bool) {
    if let Some(share) = raft_share_for_node(node_id) {
        share.set_sink_paused(paused);
    }
}

/// Scans the worker log and returns its next LSN (highest frame LSN + 1,
/// or 0 when empty) plus the trailing not-yet-terminated entry frames (seed
/// for XL reassembly; empty when the log ends at an entry boundary, which
/// is always the case for a tx-aligned committed prefix).
pub async fn scan_worker_log_tail(
    log: &::wal::worker_log::ChunkedWorkerLogBackend,
) -> RS<(u64, Vec<Vec<u8>>, Option<LSN>)> {
    use ::wal::worker_log::{scan_valid_frame_prefix, WorkerLogBackend};
    use ::wal::xl_batch::decode_xl_batches_with_pending;
    let fs = log.fs();
    let mut pending = Vec::new();
    let mut pending_start = None;
    let mut max_lsn: Option<LSN> = None;
    for path in log.chunk_paths_sorted().await? {
        let bytes = fs.read_all(&path).await?;
        if bytes.is_empty() {
            continue;
        }
        let prefix = scan_valid_frame_prefix(&bytes);
        if prefix.corrupt_reason.is_some() {
            tracing::warn!(
                path = %path.display(),
                "worker log scan dropped un-persisted tail"
            );
        }
        decode_xl_batches_with_pending(&prefix.frames, &mut pending, &mut pending_start)?;
        if let Some(lsn) = prefix.max_lsn {
            max_lsn = Some(max_lsn.map_or(lsn, |current| current.max(lsn)));
        }
    }
    Ok((
        max_lsn.map_or(0, |lsn| lsn.as_u64().saturating_add(1)),
        pending,
        pending_start,
    ))
}

/// Reads the next whole frame from a replication stream, buffering partial
/// reads. Returns `None` on clean EOF or when `stop_rx` fires.
pub async fn read_next_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    read_buf: &mut Vec<u8>,
    chunk: &mut [u8],
    stop_rx: &mut StopRx,
) -> RS<Option<Frame>> {
    loop {
        if let Some((frame, consumed)) = try_decode_next_frame(read_buf)? {
            read_buf.drain(0..consumed);
            return Ok(Some(frame));
        }
        if stop_rx.is_stopped() {
            return Ok(None);
        }
        let read = tokio::select! {
            read_result = reader.read(chunk) => {
                read_result.map_err(|e| mudu_error!(ErrorCode::Network, "read replication stream error", e))?
            }
            _ = stop_rx.changed() => {
                return Ok(None);
            }
        };
        if read == 0 {
            return Ok(None);
        }
        read_buf.extend_from_slice(&chunk[..read]);
    }
}
