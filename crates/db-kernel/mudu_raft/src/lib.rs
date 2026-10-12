//! `mudu_raft` — Raft consensus for MuduDB replication.
//!
//! This crate is a vendored fork/port of
//! [`scuptio/scupt-raft`](https://github.com/scuptio/scupt-raft)
//! (Apache-2.0, TLA+-verified Raft) restructured onto MuduDB's architecture:
//!
//! - [`core`] — the pure, synchronous consensus state machine (port of
//!   scupt-raft's `state_machine_inner`); no I/O, no locks, no wall clock.
//! - [`store`] — persistent raft log + hard state in the MuduDB log frame
//!   format v1 (byte-compatible with the kernel WAL, implemented locally).
//! - [`node`] — one event-loop thread per node plus one persist thread.
//! - [`transport`] — outbound message abstraction; the core never does
//!   network I/O.
//!
//! Public entry points: [`RaftCfg`], [`RaftHandle`], [`RaftTransport`],
//! [`InMemoryNetwork`], [`ReplEntry`].

#![warn(missing_docs)]

pub mod core;
mod entry;
pub mod frame;
mod node;
pub mod store;
pub mod transport;

#[cfg(test)]
mod cluster_test;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mudu::common::result::RS;
use mudu::error::{ErrorCode, MuduError};
use mudu::mudu_error;
use mudu_sys::sync::sync_::channel::sync_channel;
use mudu_sys::sync::sync_::unbounded_channel::SyncReceiver;
use mudu_sys::sync::{SMutex, SRwLock};
use mudu_sys::task::sync::{spawn_thread_named, SJoinHandle};

/// The replicated entry type carried by the raft log.
pub use entry::ReplEntry;

/// Cluster membership types: node id, per-node info, configuration value,
/// configuration version, and a node set paired with its value.
pub use crate::core::conf::{ConfNodeValue, ConfValue, ConfVersion, NodeInfo, NID};
/// Wire messages exchanged between raft nodes, and the `(term, index)` log
/// position.
pub use crate::core::message::{RaftMsg, TermIndex};
/// Role of a raft node.
pub use crate::core::role::RaftRole;
/// Persistent raft log store, its configuration, and the raft log directory
/// helper.
pub use crate::store::{raft_log_dir, RaftStore, StoreCfg};
/// Outbound message transport abstraction and the in-process network used by
/// tests.
pub use crate::transport::{InMemoryNetwork, RaftTransport};

use crate::core::{CoreRecovered, Effects, RaftCore};
use crate::node::{spawn_persist_thread, RaftIn, RaftNode};

/// Node configuration.
pub struct RaftCfg {
    /// This node's id.
    pub node_id: u64,
    /// All voter node ids of the cluster, **including** this node.
    ///
    /// Used only on first bootstrap: afterwards the configuration stored in
    /// the raft log wins.
    pub peers: Vec<u64>,
    /// Duration of one raft tick in milliseconds. The event loop wakes at
    /// most once per tick; election timeouts are counted in ticks, so the
    /// consensus core never reads a wall clock.
    pub raft_tick_ms: u64,
    /// Election timeout counted in ticks (pre-vote starts when a follower's
    /// tick counter exceeds this value).
    pub election_timeout_ticks: u64,
    /// Local election-timeout phasing in ticks, added on top of the conf's
    /// replicated `timeout_max_tick`. Unlike the conf value it never
    /// travels with configuration updates, so per-node election phasing
    /// survives membership changes. Default 0 (no phasing).
    pub election_timeout_offset: u64,
    /// Directory for the raft log; the store uses `<log_dir>/raft_log/`.
    pub log_dir: PathBuf,
    /// When true, the event loop does not tick on its own; time advances
    /// only via [`RaftHandle::inject_tick`] (deterministic tests).
    pub manual_ticks: bool,
    /// Optional tuning knobs.
    pub tuning: RaftTuning,
}

impl RaftCfg {
    /// Creates a configuration with default tuning and automatic ticks.
    pub fn new(
        node_id: u64,
        peers: Vec<u64>,
        raft_tick_ms: u64,
        election_timeout_ticks: u64,
        log_dir: PathBuf,
    ) -> Self {
        Self {
            node_id,
            peers,
            raft_tick_ms,
            election_timeout_ticks,
            election_timeout_offset: 0,
            log_dir,
            manual_ticks: false,
            tuning: RaftTuning::default(),
        }
    }

    /// Sets the local election-timeout phasing in ticks (see the field).
    pub fn with_election_timeout_offset(mut self, offset_ticks: u64) -> Self {
        self.election_timeout_offset = offset_ticks;
        self
    }
}

/// Optional tuning knobs of a raft node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RaftTuning {
    /// Maximum number of log entries sent in one AppendReq.
    ///
    /// Deviation from upstream scupt-raft (which used 2): a larger batch
    /// makes catch-up replication practical.
    pub max_append_entries: u64,
    /// Chunk file size limit of the raft log store.
    pub chunk_size: u64,
    /// Maximum size of one log frame part.
    pub max_part_size: usize,
    /// Threshold of the *automatic* (tick-driven) log compaction: the log
    /// is compacted up to the commit index once the commit index is at
    /// least this many entries beyond the snapshot frontier. Default:
    /// `u64::MAX` (automatic compaction disabled).
    ///
    /// Compaction happens through three independent paths regardless of
    /// this gate:
    ///
    /// - **automatic**: tick-driven, gated by this threshold (upstream
    ///   scupt-raft compacted on every long tick; the gate exists because
    ///   a follower compacted behind the leader's snapshot frontier can
    ///   only be healed through an out-of-band snapshot install);
    /// - **forced**: [`RaftHandle::force_compact`] compacts to the current
    ///   commit index immediately, ignoring the threshold;
    /// - **install**: [`RaftHandle::install_snapshot`] moves the frontier
    ///   to an externally produced snapshot.
    ///
    /// All three paths physically reclaim whole store chunks that fall
    /// entirely below the new frontier.
    pub compact_min_entries: u64,
    /// Whether to fsync every persisted write batch.
    pub fsync: bool,
}

impl Default for RaftTuning {
    fn default() -> Self {
        Self {
            max_append_entries: 64,
            chunk_size: store::DEFAULT_CHUNK_SIZE,
            max_part_size: store::DEFAULT_MAX_PART_SIZE,
            compact_min_entries: u64::MAX,
            fsync: true,
        }
    }
}

/// A snapshot of a node's observable raft state.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NodeStatus {
    /// Node id.
    pub node_id: u64,
    /// Current role.
    pub role: RaftRole,
    /// Current term.
    pub term: u64,
    /// Currently known leader, if any.
    pub leader_id: Option<u64>,
    /// Current commit index.
    pub commit_index: u64,
    /// Version of the committed configuration. While this differs from
    /// `conf_version_new`, a re-configuration is in progress.
    pub conf_version_committed: ConfVersion,
    /// Version of the new (joint) configuration.
    pub conf_version_new: ConfVersion,
}

/// Error of a failed proposal.
///
/// Carries an optional leader hint for `NotLeader` failures so callers can
/// retry on the right node. Converts into [`MuduError`] (hint is embedded
/// in the message).
#[derive(Debug)]
pub struct ProposeError {
    error: MuduError,
    leader_hint: Option<u64>,
}

impl ProposeError {
    pub(crate) fn not_leader(leader_hint: Option<u64>) -> Self {
        let message = match leader_hint {
            Some(id) => format!("not the raft leader (leader hint: {id})"),
            None => "not the raft leader (leader unknown)".to_string(),
        };
        Self {
            error: mudu_error!(ErrorCode::InvalidState, message),
            leader_hint,
        }
    }

    pub(crate) fn stopped() -> Self {
        Self {
            error: mudu_error!(
                ErrorCode::InvalidState,
                "raft node stopped before the proposal completed"
            ),
            leader_hint: None,
        }
    }

    /// The leader hint carried by `NotLeader` errors.
    pub fn leader_hint(&self) -> Option<u64> {
        self.leader_hint
    }

    /// The error code.
    pub fn ec(&self) -> ErrorCode {
        self.error.ec()
    }

    /// Converts into a plain [`MuduError`].
    pub fn into_mudu_error(self) -> MuduError {
        self.error
    }

    /// The underlying error reference.
    pub fn mudu_error(&self) -> &MuduError {
        &self.error
    }
}

impl std::fmt::Display for ProposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for ProposeError {}

impl From<ProposeError> for MuduError {
    fn from(e: ProposeError) -> Self {
        e.into_mudu_error()
    }
}

/// Result delivered to a [`RaftProposeWaiter`]: the `(index, term)` of the
/// committed entry, or a [`ProposeError`].
pub type ProposeResult = Result<(u64, u64), ProposeError>;

/// Waiter for the completion of one proposal.
///
/// Completed when the proposed entry commits (anywhere on a quorum, as
/// observed by this node), or with an error when leadership is lost or the
/// node stops.
pub struct RaftProposeWaiter {
    rx: SyncReceiver<ProposeResult>,
}

impl RaftProposeWaiter {
    /// Blocks until the proposal completes.
    pub fn recv(&self) -> ProposeResult {
        match self.rx.recv() {
            Ok(r) => r,
            Err(e) => Err(ProposeError {
                error: e,
                leader_hint: None,
            }),
        }
    }

    /// Blocks up to `dur` for the proposal to complete; `Ok(None)` on
    /// timeout.
    pub fn recv_timeout(&self, dur: Duration) -> RS<Option<ProposeResult>> {
        self.rx.recv_timeout(dur)
    }
}

/// Error of a failed re-configuration request.
///
/// Immediate rejections (not the leader, a re-configuration already in
/// progress, invalid membership) are returned by
/// [`RaftHandle::update_conf`] itself; asynchronous failures (leadership
/// lost mid-reconfiguration, node stopped) are delivered through
/// [`UpdateConfWaiter`]. `NotLeader`-style errors carry an optional
/// leader hint.
#[derive(Debug)]
pub struct UpdateConfError {
    error: MuduError,
    leader_hint: Option<u64>,
}

impl UpdateConfError {
    pub(crate) fn not_leader(leader_hint: Option<u64>) -> Self {
        let message = match leader_hint {
            Some(id) => format!("not the raft leader (leader hint: {id})"),
            None => "not the raft leader (leader unknown)".to_string(),
        };
        Self {
            error: mudu_error!(ErrorCode::InvalidState, message),
            leader_hint,
        }
    }

    pub(crate) fn reconfig_in_progress() -> Self {
        Self {
            error: mudu_error!(
                ErrorCode::InvalidState,
                "a raft re-configuration is already in progress"
            ),
            leader_hint: None,
        }
    }

    pub(crate) fn leadership_lost(leader_hint: Option<u64>) -> Self {
        let message = match leader_hint {
            Some(id) => format!(
                "raft leadership was lost while the re-configuration was in progress (leader hint: {id})"
            ),
            None => {
                "raft leadership was lost while the re-configuration was in progress".to_string()
            }
        };
        Self {
            error: mudu_error!(ErrorCode::InvalidState, message),
            leader_hint,
        }
    }

    pub(crate) fn stopped() -> Self {
        Self {
            error: mudu_error!(
                ErrorCode::InvalidState,
                "raft node stopped before the re-configuration completed"
            ),
            leader_hint: None,
        }
    }

    pub(crate) fn from_error(error: MuduError) -> Self {
        Self {
            error,
            leader_hint: None,
        }
    }

    /// The leader hint carried by `NotLeader`-style errors.
    pub fn leader_hint(&self) -> Option<u64> {
        self.leader_hint
    }

    /// The error code.
    pub fn ec(&self) -> ErrorCode {
        self.error.ec()
    }

    /// Converts into a plain [`MuduError`].
    pub fn into_mudu_error(self) -> MuduError {
        self.error
    }

    /// The underlying error reference.
    pub fn mudu_error(&self) -> &MuduError {
        &self.error
    }
}

impl std::fmt::Display for UpdateConfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for UpdateConfError {}

impl From<UpdateConfError> for MuduError {
    fn from(e: UpdateConfError) -> Self {
        e.into_mudu_error()
    }
}

/// Result delivered to a [`UpdateConfWaiter`]: the committed
/// configuration version once the re-configuration completes.
pub type UpdateConfResult = Result<ConfVersion, UpdateConfError>;

/// Waiter for the completion of one re-configuration.
///
/// Created by an accepted [`RaftHandle::update_conf`] call. Resolves with
/// the committed configuration version once the committed configuration
/// reaches the version stamped at begin time, or with an
/// [`UpdateConfError`] when leadership is lost mid-reconfiguration or the
/// node stops. (A new leader elected mid-reconfiguration keeps driving
/// the joint consensus from its own state, but this waiter does not
/// migrate: it fails fast and the caller may re-issue or re-await on the
/// new leader.)
pub struct UpdateConfWaiter {
    version: ConfVersion,
    rx: SyncReceiver<UpdateConfResult>,
}

impl std::fmt::Debug for UpdateConfWaiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateConfWaiter")
            .field("version", &self.version)
            .finish()
    }
}

impl UpdateConfWaiter {
    /// The new-configuration version this waiter tracks.
    pub fn version(&self) -> &ConfVersion {
        &self.version
    }

    /// Blocks until the re-configuration completes or fails.
    pub fn recv(&self) -> UpdateConfResult {
        match self.rx.recv() {
            Ok(r) => r,
            Err(e) => Err(UpdateConfError {
                error: e,
                leader_hint: None,
            }),
        }
    }

    /// Blocks up to `dur`; `Ok(None)` on timeout (the waiter stays valid
    /// and can be awaited again).
    pub fn recv_timeout(&self, dur: Duration) -> RS<Option<UpdateConfResult>> {
        self.rx.recv_timeout(dur)
    }
}

/// Handle of a running raft node.
pub struct RaftHandle {
    node_id: u64,
    in_tx: mudu_sys::ChannelSender<RaftIn>,
    status: Arc<SRwLock<NodeStatus>>,
    join: SMutex<Option<SJoinHandle<()>>>,
}

impl RaftHandle {
    /// Starts a raft node: opens (and if necessary bootstraps) the store,
    /// recovers the core, and spawns the event-loop and persist threads.
    ///
    /// On first start the cluster bootstraps with `cfg.peers` as the static
    /// voter set; on later starts the configuration stored in the raft log
    /// wins. `cfg.election_timeout_ticks` always applies (it is a local
    /// tunable).
    pub fn start(cfg: RaftCfg, transport: Arc<dyn RaftTransport>) -> RS<Self> {
        if cfg.peers.is_empty() || !cfg.peers.contains(&cfg.node_id) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "raft peers must be non-empty and contain the node id"
            ));
        }
        if cfg.election_timeout_ticks == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "raft election_timeout_ticks must be >= 1"
            ));
        }
        if !cfg.manual_ticks && cfg.raft_tick_ms == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "raft raft_tick_ms must be >= 1 when ticks are automatic"
            ));
        }

        let store_cfg = StoreCfg {
            chunk_size: cfg.tuning.chunk_size,
            max_part_size: cfg.tuning.max_part_size,
            fsync: cfg.tuning.fsync,
        };
        let dir = raft_log_dir(&cfg.log_dir);
        let (store, recovered) = RaftStore::open(&dir, store_cfg)?;
        let (store, recovered) = match recovered {
            Some(r) => (store, r),
            None => bootstrap_store(store, &cfg)?,
        };

        let mut core = RaftCore::new(
            cfg.tuning.max_append_entries,
            cfg.tuning.compact_min_entries,
        );
        core.recover(recovered);
        core.override_timeout_max_tick(cfg.election_timeout_ticks);
        core.set_election_timeout_offset(cfg.election_timeout_offset);

        let persist = spawn_persist_thread(cfg.node_id, store)?;
        let (in_tx, in_rx) = mudu_sys::unbounded_channel();
        let status = Arc::new(SRwLock::new(NodeStatus {
            node_id: cfg.node_id,
            role: core.role(),
            term: core.term(),
            leader_id: core.leader_id(),
            commit_index: core.commit_index(),
            conf_version_committed: core
                .raft_conf()
                .conf_node_value_committed()
                .term_version()
                .clone(),
            conf_version_new: core
                .raft_conf()
                .conf_node_value_new()
                .term_version()
                .clone(),
        }));
        let node = RaftNode::new(
            core,
            in_rx,
            transport,
            persist,
            status.clone(),
            Duration::from_millis(cfg.raft_tick_ms),
            cfg.manual_ticks,
        )?;
        let node_id = cfg.node_id;
        let join = spawn_thread_named(format!("mudu_raft_node_{node_id}"), move || {
            if let Err(e) = node.run() {
                tracing::error!("raft node {node_id} event loop exited with error: {e}");
            }
        })?;
        Ok(Self {
            node_id,
            in_tx,
            status,
            join: SMutex::new(Some(join)),
        })
    }

    /// This node's id.
    pub fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Proposes an entry for replication.
    ///
    /// The returned waiter completes with the `(index, term)` of the entry
    /// once it commits, or with a [`ProposeError`] (`NotLeader` errors carry
    /// an `Option<u64>` leader hint).
    pub fn propose(&self, entry: ReplEntry) -> RS<RaftProposeWaiter> {
        let (tx, rx) = sync_channel(1);
        self.in_tx.send(RaftIn::Propose { entry, resp: tx })?;
        Ok(RaftProposeWaiter { rx })
    }

    /// Delivers an inbound raft message to the node (transport ingress).
    pub fn deliver(&self, from: u64, msg: RaftMsg) -> RS<()> {
        self.in_tx.send(RaftIn::Net { from, msg })
    }

    /// The node's input channel, for transports that deliver by channel
    /// (e.g. [`InMemoryNetwork`]).
    pub fn input_sender(&self) -> mudu_sys::ChannelSender<RaftIn> {
        self.in_tx.clone()
    }

    /// Subscribes to committed entries `(index, entry)` as observed by this
    /// node.
    pub fn watch_committed(&self) -> mudu_sys::SyncReceiver<(u64, ReplEntry)> {
        let (tx, rx) = mudu_sys::unbounded_channel();
        let _ = self.in_tx.send(RaftIn::WatchCommits { tx });
        rx
    }

    /// Subscribes to node status changes; the current status is sent
    /// immediately.
    pub fn watch_status(&self) -> mudu_sys::SyncReceiver<NodeStatus> {
        let (tx, rx) = mudu_sys::unbounded_channel();
        let _ = self.in_tx.send(RaftIn::WatchStatus { tx });
        rx
    }

    /// Injects one tick (drives time when `manual_ticks` is set).
    pub fn inject_tick(&self) -> RS<()> {
        self.in_tx.send(RaftIn::Tick)
    }

    /// Installs an externally produced snapshot frontier.
    ///
    /// The snapshot bytes are transported by the caller (the kernel ships
    /// them over its own replication channel); this call moves the raft
    /// log frontier: the log is left-truncated at `frontier.index`, the
    /// commit index advances to at least `frontier.index`, and the store
    /// physically reclaims chunks that fall entirely below the frontier.
    ///
    /// Blocks until the frontier is durable (the store batch is fsynced
    /// before this call returns). Idempotent. A frontier beyond the last
    /// log index is accepted: the external snapshot vouches for entries the
    /// node never had (fresh node joining a compacted group), so the local
    /// log tail is discarded and the commit index is pinned to the
    /// frontier.
    pub fn install_snapshot(&self, frontier: TermIndex) -> RS<()> {
        let (tx, rx) = sync_channel(1);
        self.in_tx
            .send(RaftIn::InstallSnapshot { frontier, resp: tx })?;
        rx.recv()?
    }

    /// Forces log compaction up to the current commit index, bypassing the
    /// `compact_min_entries` gate of [`RaftTuning`].
    ///
    /// Intended to be called after installing a business snapshot so the
    /// raft log is compacted (and physically reclaimed) in lockstep.
    /// Blocks until the compaction is durable.
    pub fn force_compact(&self) -> RS<()> {
        let (tx, rx) = sync_channel(1);
        self.in_tx.send(RaftIn::ForceCompact { resp: tx })?;
        rx.recv()?
    }

    /// Begins a joint-consensus re-configuration (leader only).
    ///
    /// Returns once the new configuration is durable on this node:
    /// `Err` immediately when this node is not the leader (the
    /// [`UpdateConfError`] carries the leader hint), when a previous
    /// re-configuration is still in progress, or when the membership is
    /// invalid (`nid_vote` must be a non-empty subset of `nid_log`).
    ///
    /// On success the returned [`UpdateConfWaiter`] resolves with the
    /// committed configuration version once the joint consensus commits
    /// the new configuration (driven by ticks and heartbeats), or fails
    /// with an explicit error when leadership is lost mid-reconfiguration
    /// or the node stops — it never hangs. The change is complete cluster
    /// -wide once the committed conf version reaches
    /// [`UpdateConfWaiter::version`]; followers adopt it through the
    /// leader's conf sync.
    ///
    /// Convenience membership derivation for the four common operations
    /// lives on [`ConfNodeValue`]: `derive_add_voter`, `derive_add_learner`,
    /// `derive_remove_node`, `derive_promote_voter`.
    pub fn update_conf(
        &self,
        conf_value: ConfValue,
        nid_vote: Vec<u64>,
        nid_log: Vec<u64>,
    ) -> Result<UpdateConfWaiter, UpdateConfError> {
        let (ack_tx, ack_rx) = sync_channel(1);
        let (done_tx, done_rx) = sync_channel(1);
        self.in_tx
            .send(RaftIn::UpdateConf {
                conf_value,
                nid_vote,
                nid_log,
                ack: ack_tx,
                done: done_tx,
            })
            .map_err(UpdateConfError::from_error)?;
        let version = ack_rx.recv().map_err(UpdateConfError::from_error)??;
        Ok(UpdateConfWaiter {
            version,
            rx: done_rx,
        })
    }

    /// Reads the current `(committed, new)` configuration pair.
    ///
    /// The committed pair is the one quorums are counted on while no
    /// re-configuration is in progress (then both are equal). Intended for
    /// upper layers synchronizing address books and membership.
    pub fn conf(&self) -> RS<(ConfNodeValue, ConfNodeValue)> {
        let (tx, rx) = sync_channel(1);
        self.in_tx.send(RaftIn::QueryConf { resp: tx })?;
        rx.recv()
    }

    /// Current role.
    pub fn role(&self) -> RaftRole {
        match self.status.read() {
            Ok(s) => s.role,
            Err(_) => RaftRole::Follower,
        }
    }

    /// Currently known leader, if any.
    pub fn leader_id(&self) -> Option<u64> {
        match self.status.read() {
            Ok(s) => s.leader_id,
            Err(_) => None,
        }
    }

    /// Current term.
    pub fn term(&self) -> u64 {
        match self.status.read() {
            Ok(s) => s.term,
            Err(_) => 0,
        }
    }

    /// Current commit index.
    pub fn commit_index(&self) -> u64 {
        match self.status.read() {
            Ok(s) => s.commit_index,
            Err(_) => 0,
        }
    }

    /// Full status snapshot.
    pub fn status(&self) -> NodeStatus {
        match self.status.read() {
            Ok(s) => s.clone(),
            Err(_) => NodeStatus {
                node_id: self.node_id,
                role: RaftRole::Follower,
                term: 0,
                leader_id: None,
                commit_index: 0,
                conf_version_committed: ConfVersion::default(),
                conf_version_new: ConfVersion::default(),
            },
        }
    }

    /// Stops the node: the event loop finishes in-flight persistence and
    /// both threads are joined. Idempotent.
    pub fn stop(&self) -> RS<()> {
        let join = self.join.lock()?.take();
        if let Some(join) = join {
            // best-effort: the loop also exits when the channel closes
            let _ = self.in_tx.send(RaftIn::Stop);
            join.join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "raft node thread panicked on stop"))?;
        }
        Ok(())
    }
}

impl Drop for RaftHandle {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.join.lock() {
            if let Some(join) = guard.take() {
                let _ = self.in_tx.send(RaftIn::Stop);
                let _ = join.join();
            }
        }
    }
}

fn bootstrap_store(mut store: RaftStore, cfg: &RaftCfg) -> RS<(RaftStore, CoreRecovered)> {
    let mut peers = cfg.peers.clone();
    peers.sort_unstable();
    peers.dedup();
    let conf_value = core::conf::ConfValue::new(
        cfg.node_id,
        cfg.election_timeout_ticks,
        peers
            .into_iter()
            .map(|node_id| core::conf::NodeInfo {
                node_id,
                can_vote: true,
            })
            .collect(),
    );
    let mut fx = Effects::default();
    RaftCore::bootstrap_effects(conf_value.clone(), &mut fx);
    store.apply_batch(&fx.writes)?;
    let recovered = RaftCore::bootstrap_recovered(conf_value);
    Ok((store, recovered))
}
