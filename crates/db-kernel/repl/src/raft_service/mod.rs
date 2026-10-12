//! Raft group lifecycle and the MSFM transport for raft traffic.
//!
//! One raft group per server (`RaftHandle`) runs over the replication
//! channel: outbound raft messages become MSFM `Raft*` frames written to
//! per-peer connections managed here; inbound `Raft*` frames arrive at the
//! repl listener ([`crate::source`]) and are delivered to the
//! raft handle via [`raft_frame_to_msg`] + `RaftHandle::deliver`.
//!
//! The [`RaftShare`] is the node's view of raft state shared with workers
//! (write gate, commit sequencer, xid allocation), the heal driver and the
//! HTTP topology endpoint.

mod codec;
mod membership;
mod tasks;
mod transport;

pub use membership::*;

pub(crate) use codec::{raft_frame_to_msg, raft_msg_to_frame_bytes};

use crate::seg_hash::SegHash;
use crate::{register_raft, unregister_raft};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::catalog::repl_cfg::{ReplNodeCfg, ReplPeerCfg};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::LeaderHint;
use mudu_raft::{NodeStatus, RaftCfg, RaftHandle, RaftRole};
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopTx};
use mudu_sys::sync::{SMutex, SRwLock};
use mudu_sys::task::sync::SJoinHandle;
use mudu_sys::tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tasks::{spawn_commit_dispatch, spawn_raft_io_thread, spawn_status_watcher};
use transport::MudbRaftTransport;

/// The node's raft state shared with workers, the heal flow and the HTTP
/// topology endpoint.
pub struct RaftShare {
    node_id: u64,
    status: SRwLock<NodeStatus>,
    term: Arc<AtomicU64>,
    serving: AtomicBool,
    committed_vector: SRwLock<BTreeMap<u32, (u64, SegHash)>>,
    /// Highest raft index delivered to the local commit dispatcher.
    delivered_index: AtomicU64,
    /// Cluster members with replication addresses (all `nid_log` members;
    /// kept in sync with the committed conf by the conf-sync task).
    peers: SRwLock<Vec<ReplPeerCfg>>,
    /// Voting member node ids (`nid_vote` of the committed conf); learners
    /// replicate but never count toward a quorum.
    voters: SRwLock<Vec<u64>>,
    client_port: u16,
    sink_paused: AtomicBool,
    heal_trigger: SMutex<Option<UnboundedSender<u64>>>,
}

impl RaftShare {
    fn new(node_id: u64, peers: Vec<ReplPeerCfg>, client_port: u16) -> Self {
        let voters = peers.iter().map(|peer| peer.node_id()).collect();
        Self {
            node_id,
            status: SRwLock::new(NodeStatus {
                node_id,
                role: RaftRole::Follower,
                term: 0,
                leader_id: None,
                commit_index: 0,
                // placeholder until the raft service feeds live conf
                // versions from the raft handle
                conf_version_committed: mudu_raft::ConfVersion::default(),
                conf_version_new: mudu_raft::ConfVersion::default(),
            }),
            term: Arc::new(AtomicU64::new(0)),
            serving: AtomicBool::new(false),
            committed_vector: SRwLock::new(BTreeMap::new()),
            delivered_index: AtomicU64::new(0),
            peers: SRwLock::new(peers),
            voters: SRwLock::new(voters),
            client_port,
            sink_paused: AtomicBool::new(false),
            heal_trigger: SMutex::new(None),
        }
    }

    /// This node's cluster id.
    pub fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Current raft role.
    pub fn role(&self) -> RaftRole {
        self.status
            .read()
            .map(|status| status.role)
            .unwrap_or(RaftRole::Follower)
    }

    /// Current raft term (cheap atomic read; feeds xid allocation).
    pub fn term(&self) -> u64 {
        self.term.load(Ordering::Relaxed)
    }

    /// The term as a shared atomic for the snapshot manager's xid
    /// allocation (`xid = (term << 32) | counter` in replication mode, so a
    /// new leader never reissues another leader's xids).
    pub(crate) fn term_source(&self) -> Arc<AtomicU64> {
        self.term.clone()
    }

    /// Currently known raft leader, if any.
    pub fn leader_id(&self) -> Option<u64> {
        self.status.read().ok().and_then(|status| status.leader_id)
    }

    /// Current raft commit index.
    pub fn commit_index(&self) -> u64 {
        self.status
            .read()
            .map(|status| status.commit_index)
            .unwrap_or(0)
    }

    /// Whether this node is the raft leader AND finished leader-elect
    /// healing (only then are client writes ungated).
    pub(crate) fn is_serving_leader(&self) -> bool {
        self.role() == RaftRole::Leader && self.is_serving()
    }

    /// Whether client writes are currently ungated on this node.
    pub fn is_serving(&self) -> bool {
        self.serving.load(Ordering::Relaxed)
    }

    /// Full status snapshot (observability endpoint).
    pub(crate) fn status_snapshot(&self) -> NodeStatus {
        self.status
            .read()
            .map(|status| status.clone())
            .unwrap_or(NodeStatus {
                node_id: self.node_id,
                role: RaftRole::Follower,
                term: 0,
                leader_id: None,
                commit_index: 0,
                conf_version_committed: mudu_raft::ConfVersion::default(),
                conf_version_new: mudu_raft::ConfVersion::default(),
            })
    }

    /// Opens or closes the write gate (heal flow).
    pub(crate) fn set_serving(&self, serving: bool) {
        self.serving.store(serving, Ordering::Relaxed);
    }

    /// The latest committed vector known locally: worker index ->
    /// `(lsn, seg_hash)` of the last delivered `CommitVec`.
    pub fn committed_vector(&self) -> BTreeMap<u32, (u64, SegHash)> {
        self.committed_vector
            .read()
            .map(|vector| vector.clone())
            .unwrap_or_default()
    }

    /// Records one worker's committed point from a delivered `CommitVec`.
    pub(crate) fn record_committed(&self, worker_index: u32, lsn: u64, hash: SegHash) {
        if let Ok(mut vector) = self.committed_vector.write() {
            let entry = vector.entry(worker_index).or_insert((0, [0u8; 32]));
            if lsn >= entry.0 {
                *entry = (lsn, hash);
            }
        }
    }

    /// Records the raft index of a committed entry delivered to the local
    /// commit dispatcher (any entry kind). The leader-elect heal waits on
    /// this reaching the node's commit index before trusting the committed
    /// vector.
    pub(crate) fn record_delivered_index(&self, index: u64) {
        self.delivered_index.fetch_max(index, Ordering::Relaxed);
    }

    /// Highest raft index delivered to the local commit dispatcher.
    pub(crate) fn delivered_index(&self) -> u64 {
        self.delivered_index.load(Ordering::Relaxed)
    }

    /// The full member list (all `nid_log` members, learners included).
    pub fn peers(&self) -> Vec<ReplPeerCfg> {
        self.peers
            .read()
            .map(|peers| peers.clone())
            .unwrap_or_default()
    }

    /// Voting member node ids of the committed conf.
    pub(crate) fn voter_ids(&self) -> Vec<u64> {
        self.voters
            .read()
            .map(|voters| voters.clone())
            .unwrap_or_default()
    }

    /// Raft quorum size of the current committed conf.
    pub(crate) fn quorum_size(&self) -> usize {
        self.voters.read().map(|v| v.len()).unwrap_or(0) / 2 + 1
    }

    /// `host:port` of a peer's replication endpoint.
    pub fn peer_addr(&self, node_id: u64) -> Option<String> {
        self.peers
            .read()
            .ok()?
            .iter()
            .find(|peer| peer.node_id() == node_id)
            .map(|peer| format!("{}:{}", peer.host(), peer.port()))
    }

    /// Replaces the member/address and voter lists after a committed conf
    /// change (conf-sync task).
    pub(crate) fn update_membership(&self, peers: Vec<ReplPeerCfg>, voters: Vec<u64>) {
        if let Ok(mut slot) = self.peers.write() {
            *slot = peers;
        }
        if let Ok(mut slot) = self.voters.write() {
            *slot = voters;
        }
    }

    /// Leader hint for client redirect: the raft leader's host with this
    /// node's client port (symmetric deployment), `None` when no leader is
    /// known or this node is the leader.
    pub(crate) fn leader_hint(&self) -> Option<LeaderHint> {
        let leader = self.leader_id()?;
        if leader == self.node_id {
            return None;
        }
        let peers = self.peers.read().ok()?;
        let peer = peers.iter().find(|peer| peer.node_id() == leader)?;
        Some(LeaderHint::new(peer.host().to_string(), self.client_port))
    }

    /// Test hook state: whether this node's sinks are paused.
    pub(crate) fn sink_paused(&self) -> bool {
        self.sink_paused.load(Ordering::Relaxed)
    }

    /// Test hook: pause or resume this node's sinks.
    pub(crate) fn set_sink_paused(&self, paused: bool) {
        self.sink_paused.store(paused, Ordering::Relaxed);
    }

    fn update_status(&self, status: &NodeStatus) {
        self.term.store(status.term, Ordering::Relaxed);
        if let Ok(mut slot) = self.status.write() {
            *slot = status.clone();
        }
    }

    fn set_heal_trigger(&self, trigger: UnboundedSender<u64>) {
        if let Ok(mut slot) = self.heal_trigger.lock() {
            *slot = Some(trigger);
        }
    }

    /// Wakes the heal driver (leader-election event).
    pub(crate) fn trigger_heal(&self) {
        if let Ok(slot) = self.heal_trigger.lock() {
            if let Some(trigger) = slot.as_ref() {
                let _ = trigger.send(self.term());
            }
        }
    }
}

impl ::catalog::repl_ctrl::RaftShareView for RaftShare {
    fn is_serving_leader(&self) -> bool {
        RaftShare::is_serving_leader(self)
    }

    fn leader_hint(&self) -> Option<LeaderHint> {
        RaftShare::leader_hint(self)
    }

    fn term_source(&self) -> Arc<AtomicU64> {
        RaftShare::term_source(self)
    }
}

/// Handle of the running raft service: the raft group, its I/O thread and
/// the status/commit dispatchers.
pub struct RaftServiceHandle {
    server_instance_id: ServerInstanceId,
    raft: Arc<RaftHandle>,
    share: Arc<RaftShare>,
    io_stop: StopTx,
    io_join: Option<SJoinHandle<RS<()>>>,
    watcher_join: Option<SJoinHandle<RS<()>>>,
    dispatch_join: Option<SJoinHandle<RS<()>>>,
}

impl RaftServiceHandle {
    /// The shared raft state.
    pub fn share(&self) -> &Arc<RaftShare> {
        &self.share
    }

    /// Stops the service: closes the write gate, stops the raft group (its
    /// event loop and persist thread exit, which ends the dispatchers), then
    /// the I/O thread. Blocks until every thread joined.
    pub fn stop(mut self) -> RS<()> {
        self.share.set_serving(false);
        self.raft.stop()?;
        self.io_stop.stop();
        if let Some(join) = self.io_join.take() {
            join.join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "join raft io thread error"))??;
        }
        if let Some(join) = self.watcher_join.take() {
            join.join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "join raft status watcher error"))??;
        }
        if let Some(join) = self.dispatch_join.take() {
            join.join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "join raft commit dispatch error"))??;
        }
        unregister_raft(self.server_instance_id)?;
        crate::raft_shares_by_node_unregister(self.share.node_id())?;
        crate::raft_handle_by_node_unregister(self.share.node_id())?;
        Ok(())
    }
}

/// Starts the raft service for this server: one raft group, one I/O thread
/// (per-peer writers + heal driver + conf sync), a status watcher and a
/// commit dispatcher. Elections begin immediately; client writes stay
/// gated until the heal flow opens them.
pub fn start_raft_service(
    server_instance_id: ServerInstanceId,
    repl_cfg: &ReplNodeCfg,
    log_dir: &str,
    client_port: u16,
) -> RS<RaftServiceHandle> {
    let share = Arc::new(RaftShare::new(
        repl_cfg.node_id(),
        repl_cfg.peers().to_vec(),
        client_port,
    ));
    // Seed the kernel-side member address book from the static
    // configuration; membership changes keep it aligned afterwards.
    for peer in repl_cfg.peers() {
        crate::repl_member_addr_register(peer.node_id(), peer.host().to_string(), peer.port())?;
    }
    let voter_ids: Vec<u64> = repl_cfg.peers().iter().map(|peer| peer.node_id()).collect();
    let (io_stop_tx, io_stop_rx) = stop_channel();
    let transport = Arc::new(MudbRaftTransport {
        node_id: repl_cfg.node_id(),
        out: SMutex::new(HashMap::new()),
    });
    let raft_cfg = RaftCfg::new(
        repl_cfg.node_id(),
        voter_ids,
        repl_cfg.raft_tick_ms(),
        repl_cfg.election_timeout_ticks(),
        PathBuf::from(log_dir),
    )
    // Deterministic per-node election phasing: the raft core's timeout is a
    // fixed tick count with no randomization, so nodes sharing the same
    // timeout phase would split every vote deterministically (lockstep
    // livelock). The offset is LOCAL (never replicated with the conf), so
    // the phasing survives membership changes.
    .with_election_timeout_offset(repl_cfg.node_id());
    // The heal trigger channel is created (and installed into the share)
    // before any thread starts, so a leader transition can never fire
    // before the heal driver is reachable.
    let (heal_tx, heal_rx) = unbounded_channel::<u64>();
    share.set_heal_trigger(heal_tx);
    let raft = Arc::new(RaftHandle::start(raft_cfg, transport.clone())?);
    register_raft(server_instance_id, raft.clone(), share.clone())?;
    crate::raft_shares_by_node_register(share.clone())?;
    crate::raft_handle_by_node_register(repl_cfg.node_id(), raft.clone())?;
    // Replace the placeholder status (and its default conf versions) with
    // the real recovered state before any watcher consumer can observe it.
    share.update_status(&raft.status());

    let io_join = spawn_raft_io_thread(
        server_instance_id,
        share.clone(),
        raft.clone(),
        transport,
        heal_rx,
        io_stop_rx,
    )?;
    let watcher_join = spawn_status_watcher(raft.clone(), share.clone())?;
    let dispatch_join = spawn_commit_dispatch(server_instance_id, raft.clone(), share.clone())?;
    Ok(RaftServiceHandle {
        server_instance_id,
        raft,
        share,
        io_stop: io_stop_tx,
        io_join: Some(io_join),
        watcher_join: Some(watcher_join),
        dispatch_join: Some(dispatch_join),
    })
}
