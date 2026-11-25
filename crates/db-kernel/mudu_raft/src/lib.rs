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
//! - `node` — one serialized async task per group on a shared owner executor.
//! - [`transport`] — outbound message abstraction; the core never does
//!   network I/O.
//!
//! Public entry points: [`RaftCfg`], [`RaftHandle`], [`RaftTransport`],
//! [`InMemoryNetwork`], [`ReplEntry`].

#![warn(missing_docs)]

mod admission;
/// Revocable fence for queued proposal certificates.
pub use admission::{AdmissionGuard, MembershipFence};
mod api;
mod completion;
pub mod core;
mod entry;
pub mod frame;
mod handle_async;
mod inbox;
mod node;
mod replay;
mod runtime;
/// Recovery page from the durable committed log.
pub use replay::{CommittedBatch, MAX_REPLAY_BYTES};
pub mod store;
pub mod stream;
pub mod transport;

#[cfg(test)]
mod cluster_test;
#[cfg(test)]
mod runtime_tests;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::completion::{completion, CompletionReceiver};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::{SMutex, SRwLock};

/// The replicated entry type carried by the raft log.
pub use entry::ReplEntry;

/// Public result and status types of a raft node.
pub use crate::api::{NodeStatus, ProposeError, ProposeResult, UpdateConfError, UpdateConfResult};

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
/// Events accepted by a driver ingress handle.
pub use crate::inbox::RaftIn;
/// Nonblocking input handle with bounded message and byte budgets.
pub use crate::inbox::RaftSender;
use crate::node::RaftNode;

/// Node configuration.
pub struct RaftCfg {
    /// This node's id.
    pub node_id: u64,
    /// All voter node ids of the cluster, **including** this node when it is
    /// a voter.
    ///
    /// Used only on first bootstrap: afterwards the configuration stored in
    /// the raft log wins.
    pub peers: Vec<u64>,
    /// All learner node ids of the cluster at bootstrap, possibly including
    /// this node. Learners replicate the log but never vote or campaign.
    ///
    /// Used only on first bootstrap (same rule as `peers`); every node of the
    /// cluster must bootstrap with the identical voter/learner split.
    pub learners: Vec<u64>,
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
            learners: Vec::new(),
            raft_tick_ms,
            election_timeout_ticks,
            election_timeout_offset: 0,
            log_dir,
            manual_ticks: false,
            tuning: RaftTuning::default(),
        }
    }

    /// Sets the bootstrap learner set (see the field). Every node of the
    /// cluster must pass the identical voter/learner split.
    pub fn with_learners(mut self, learners: Vec<u64>) -> Self {
        self.learners = learners;
        self
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

/// Waiter for the completion of one proposal.
///
/// Completed when the proposed entry commits (anywhere on a quorum, as
/// observed by this node), or with an error when leadership is lost or the
/// node stops.
pub struct RaftProposeWaiter {
    rx: CompletionReceiver<ProposeResult>,
}

impl RaftProposeWaiter {
    /// Waits for commit or failure without blocking an executor thread.
    pub async fn wait(mut self) -> ProposeResult {
        self.rx.wait().await.map_err(ProposeError::from_error)?
    }

    /// Blocks until the proposal completes.
    pub fn recv(&self) -> ProposeResult {
        runtime::ensure_sync_wait().map_err(ProposeError::from_error)?;
        match self.rx.recv() {
            Ok(r) => r,
            Err(e) => Err(ProposeError::from_error(e)),
        }
    }

    /// Blocks up to `dur` for the proposal to complete; `Ok(None)` on
    /// timeout.
    pub fn recv_timeout(&self, dur: Duration) -> RS<Option<ProposeResult>> {
        runtime::ensure_sync_wait()?;
        self.rx.recv_timeout(dur)
    }
}

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
    rx: CompletionReceiver<UpdateConfResult>,
}

impl std::fmt::Debug for UpdateConfWaiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateConfWaiter")
            .field("version", &self.version)
            .finish()
    }
}

impl UpdateConfWaiter {
    /// Waits for the configuration to commit without blocking an executor.
    pub async fn wait(mut self) -> UpdateConfResult {
        self.rx.wait().await.map_err(UpdateConfError::from_error)?
    }

    /// The new-configuration version this waiter tracks.
    pub fn version(&self) -> &ConfVersion {
        &self.version
    }

    /// Blocks until the re-configuration completes or fails.
    pub fn recv(&self) -> UpdateConfResult {
        runtime::ensure_sync_wait().map_err(UpdateConfError::from_error)?;
        match self.rx.recv() {
            Ok(r) => r,
            Err(e) => Err(UpdateConfError::from_error(e)),
        }
    }

    /// Blocks up to `dur`; `Ok(None)` on timeout (the waiter stays valid
    /// and can be awaited again).
    pub fn recv_timeout(&self, dur: Duration) -> RS<Option<UpdateConfResult>> {
        runtime::ensure_sync_wait()?;
        self.rx.recv_timeout(dur)
    }
}

/// Handle of a running raft node.
pub struct RaftHandle {
    node_id: u64,
    in_tx: RaftSender,
    status: Arc<SRwLock<NodeStatus>>,
    join: SMutex<Option<CompletionReceiver<RS<()>>>>,
    stopped: mudu_sys::tokio::sync::watch::Receiver<Option<RS<()>>>,
}

impl RaftHandle {
    /// Starts a raft node: opens (and if necessary bootstraps) the store,
    /// recovers the core, and starts its serialized asynchronous driver.
    ///
    /// On first start the cluster bootstraps with `cfg.peers` as the static
    /// voter set and `cfg.learners` as the static learner set; on later
    /// starts the configuration stored in the raft log wins.
    /// `cfg.election_timeout_ticks` always applies (it is a local tunable).
    pub fn start(cfg: RaftCfg, transport: Arc<dyn RaftTransport>) -> RS<Self> {
        runtime::ensure_sync_wait()?;
        Self::start_waiter(cfg, transport)?.recv()?
    }

    /// Starts a node without blocking the caller's executor during recovery.
    pub async fn start_async(cfg: RaftCfg, transport: Arc<dyn RaftTransport>) -> RS<Self> {
        Self::start_waiter(cfg, transport)?.wait().await?
    }

    fn start_waiter(
        cfg: RaftCfg,
        transport: Arc<dyn RaftTransport>,
    ) -> RS<CompletionReceiver<RS<Self>>> {
        let (ready, wait) = completion();
        runtime::spawn(async move {
            let _ = ready.send(Self::start_on_runtime(cfg, transport).await);
        })?;
        Ok(wait)
    }

    async fn start_on_runtime(cfg: RaftCfg, transport: Arc<dyn RaftTransport>) -> RS<Self> {
        if cfg.peers.is_empty()
            || (!cfg.peers.contains(&cfg.node_id) && !cfg.learners.contains(&cfg.node_id))
        {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "raft peers must be non-empty and contain the node id as voter or learner"
            ));
        }
        if cfg.peers.iter().any(|peer| cfg.learners.contains(peer)) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "raft voters and learners must be disjoint"
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
        let (store, recovered) = RaftStore::open_async(&dir, store_cfg).await?;
        let (store, recovered) = match recovered {
            Some(r) => (store, r),
            None => bootstrap_store_async(store, &cfg).await?,
        };

        let mut core = RaftCore::new(
            cfg.tuning.max_append_entries,
            cfg.tuning.compact_min_entries,
        );
        core.recover(recovered);
        core.override_timeout_max_tick(cfg.election_timeout_ticks);
        core.set_election_timeout_offset(cfg.election_timeout_offset);

        let (in_tx, in_rx) = inbox::inbox();
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
            Box::new(store),
            status.clone(),
            Duration::from_millis(cfg.raft_tick_ms),
            cfg.manual_ticks,
        )?;
        let node_id = cfg.node_id;
        let (done, join) = completion();
        let (finished, stopped) = mudu_sys::tokio::sync::watch::channel(None);
        runtime::spawn(async move {
            let result = node.run().await;
            if let Err(e) = &result {
                tracing::error!("raft node {node_id} event loop exited with error: {e}");
            }
            finished.send_replace(Some(result.clone()));
            let _ = done.send(result);
        })?;
        Ok(Self {
            node_id,
            in_tx,
            status,
            join: SMutex::new(Some(join)),
            stopped,
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
        let (tx, rx) = completion();
        self.in_tx.send(RaftIn::Propose {
            entry,
            resp: tx,
            expected_conf: None,
            expected_term: None,
            guards: Vec::new(),
        })?;
        Ok(RaftProposeWaiter { rx })
    }

    /// Delivers an inbound raft message to the node (transport ingress).
    pub fn deliver(&self, from: u64, msg: RaftMsg) -> RS<()> {
        self.in_tx.send(RaftIn::Net { from, msg })
    }

    /// The node's input channel, for transports that deliver by channel
    /// (e.g. [`InMemoryNetwork`]).
    pub fn input_sender(&self) -> RaftSender {
        self.in_tx.clone()
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
        runtime::ensure_sync_wait()?;
        let (tx, rx) = completion();
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
        runtime::ensure_sync_wait()?;
        let (tx, rx) = completion();
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
        runtime::ensure_sync_wait().map_err(UpdateConfError::from_error)?;
        let (ack_tx, ack_rx) = completion();
        let (done_tx, done_rx) = completion();
        self.in_tx
            .send(RaftIn::UpdateConf {
                conf_value,
                nid_vote,
                nid_log,
                fence: None,
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
        runtime::ensure_sync_wait()?;
        let (tx, rx) = completion();
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
    /// its asynchronous driver is drained. Idempotent.
    pub fn stop(&self) -> RS<()> {
        runtime::ensure_sync_wait()?;
        let join = self.join.lock()?.take();
        if let Some(join) = join {
            // best-effort: the loop also exits when the channel closes
            let _ = self.in_tx.send(RaftIn::Stop);
            join.recv()??;
        }
        Ok(())
    }
}

impl Drop for RaftHandle {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.join.lock() {
            if let Some(join) = guard.take() {
                let _ = self.in_tx.send(RaftIn::Stop);
                // Dropping an async startup result can run on the owner
                // itself. Never synchronously join from an async runtime.
                if !mudu_sys::task::async_::has_tokio_runtime() {
                    let _ = join.recv();
                }
            }
        }
    }
}

async fn bootstrap_store_async(
    mut store: RaftStore,
    cfg: &RaftCfg,
) -> RS<(RaftStore, CoreRecovered)> {
    let mut peers = cfg.peers.clone();
    peers.sort_unstable();
    peers.dedup();
    let mut learners = cfg.learners.clone();
    learners.sort_unstable();
    learners.dedup();
    let value = ConfValue::new(
        cfg.node_id,
        cfg.election_timeout_ticks,
        peers
            .into_iter()
            .map(|node_id| NodeInfo {
                node_id,
                can_vote: true,
            })
            .chain(learners.into_iter().map(|node_id| NodeInfo {
                node_id,
                can_vote: false,
            }))
            .collect(),
    );
    let mut fx = Effects::default();
    RaftCore::bootstrap_effects(value.clone(), &mut fx);
    store.apply_batch_async(&fx.writes).await?;
    Ok((store, RaftCore::bootstrap_recovered(value)))
}
