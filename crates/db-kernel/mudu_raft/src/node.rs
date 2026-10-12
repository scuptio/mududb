//! The single-threaded raft event loop.
//!
//! One thread per node drives the pure [`RaftCore`]:
//!
//! ```text
//! loop {
//!     match in_rx.recv_timeout(tick_dur) {
//!         Ok(RaftIn::Net { from, msg })  => core.step_incoming(.., &mut fx),
//!         Ok(RaftIn::Propose { .. })     => core.step_propose(.., &mut fx),
//!         Ok(RaftIn::InstallSnapshot { .. }) => core.step_install_snapshot(.., &mut fx),
//!         Ok(RaftIn::ForceCompact { .. })    => core.step_force_compact(&mut fx),
//!         Ok(RaftIn::Tick) | timeout     => core.step_tick(&mut fx),
//!         Ok(RaftIn::Stop)               => break + persist,
//!     }
//!     persist(fx.writes) -> wait durable; transport.send(fx.messages);
//!     complete waiters + watchers with fx.committed;
//!     (snapshot/compact requests are answered only after persistence)
//! }
//! ```
//!
//! Persistence runs on a separate thread that applies write batches to the
//! [`RaftStore`] (fsync per batch by default); the event loop waits for the
//! durability acknowledgement of a batch *before* sending the messages
//! produced by the same step, preserving raft's write-before-respond rule.
//!
//! Time enters the loop only as ticks: with `manual_ticks = false` each
//! `recv_timeout` expiry is one tick (`cfg.raft_tick_ms` per tick); with
//! `manual_ticks = true` the loop parks on `recv()` and only injected
//! [`RaftIn::Tick`] messages advance time (deterministic tests).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mudu::common::result::RS;
use mudu::error::{ErrorCode, MuduError};
use mudu::mudu_error;
use mudu_sys::sync::sync_::channel::{sync_channel, SyncSender};
use mudu_sys::sync::sync_::unbounded_channel::{ChannelSender, SyncReceiver};
use mudu_sys::sync::SRwLock;
use mudu_sys::task::sync::{spawn_thread_named, SJoinHandle};
use tracing::{debug, error, trace, warn};

use crate::core::conf::{ConfNodeValue, ConfValue, ConfVersion, NID};
use crate::core::message::{RaftMsg, TermIndex, WriteOp};
use crate::core::{Effects, ProposeOutcome, RaftCore, ReConfOutcome};
use crate::entry::ReplEntry;
use crate::store::RaftStore;
use crate::transport::RaftTransport;
use crate::{NodeStatus, ProposeError, ProposeResult, UpdateConfError, UpdateConfResult};

/// Input message of a raft node's event loop.
pub enum RaftIn {
    /// An inbound raft message from a peer.
    Net {
        /// Sender node id.
        from: NID,
        /// The raft message.
        msg: RaftMsg,
    },
    /// A local proposal request.
    Propose {
        /// Entry to replicate.
        entry: ReplEntry,
        /// Completion channel of the waiter.
        resp: SyncSender<ProposeResult>,
    },
    /// Register a committed-entry watcher.
    WatchCommits {
        /// Watcher channel.
        tx: ChannelSender<(u64, ReplEntry)>,
    },
    /// Register a status watcher.
    WatchStatus {
        /// Watcher channel.
        tx: ChannelSender<NodeStatus>,
    },
    /// Install an externally produced snapshot frontier.
    InstallSnapshot {
        /// The snapshot frontier `(term, index)`; the log is
        /// left-truncated at `index`.
        frontier: TermIndex,
        /// Completed after the frontier is durable.
        resp: SyncSender<RS<()>>,
    },
    /// Force log compaction up to the current commit index.
    ForceCompact {
        /// Completed after the compaction is durable.
        resp: SyncSender<RS<()>>,
    },
    /// Begin a joint-consensus re-configuration (leader only).
    UpdateConf {
        /// Membership value of the new configuration.
        conf_value: ConfValue,
        /// Voting nodes of the new configuration (subset of `nid_log`).
        nid_vote: Vec<NID>,
        /// Log-replicating nodes of the new configuration.
        nid_log: Vec<NID>,
        /// Acknowledged (accepted + durable, or the rejection reason)
        /// after the new configuration is persisted.
        ack: SyncSender<Result<ConfVersion, UpdateConfError>>,
        /// Completed when the re-configuration commits (or fails when
        /// leadership is lost or the node stops).
        done: SyncSender<UpdateConfResult>,
    },
    /// Read the current committed and new configuration.
    QueryConf {
        /// Receives `(committed, new)` immediately.
        resp: SyncSender<(ConfNodeValue, ConfNodeValue)>,
    },
    /// An injected tick (drives time when `manual_ticks` is set).
    Tick,
    /// Stop the event loop (persists everything first).
    Stop,
}

pub(crate) struct PersistReq {
    pub(crate) ops: Vec<WriteOp>,
    pub(crate) ack: SyncSender<RS<()>>,
}

/// A deferred request response: sent only after the effects of the same
/// step are durable. Carries the response channel, the step result, and an
/// optional log index through which pending proposals must be completed.
type DeferredResp = (SyncSender<RS<()>>, RS<()>, Option<u64>);

/// A deferred re-configuration acknowledgement: the accept/reject ack is
/// sent only after the new configuration is durable; on acceptance the
/// completion channel is parked in `pending_reconf` until the committed
/// configuration reaches the stamped version.
type DeferredReConf = (
    SyncSender<Result<ConfVersion, UpdateConfError>>,
    SyncSender<UpdateConfResult>,
    RS<ReConfOutcome>,
);

/// The persist thread handle: its request channel and join handle.
pub(crate) struct PersistHandle {
    tx: ChannelSender<PersistReq>,
    join: SJoinHandle<()>,
}

/// Spawns the persist thread that owns the store.
///
/// Applies write batches in order, fsyncing each batch (per store config),
/// and acknowledges every batch. Exits when the request channel closes.
pub(crate) fn spawn_persist_thread(node_id: NID, mut store: RaftStore) -> RS<PersistHandle> {
    let (tx, rx) = mudu_sys::unbounded_channel::<PersistReq>();
    let join = spawn_thread_named(format!("mudu_raft_persist_{node_id}"), move || loop {
        match rx.recv() {
            Ok(req) => {
                let r = store.apply_batch(&req.ops);
                if let Err(e) = &r {
                    error!("raft persist thread {node_id}: apply batch error: {e}");
                }
                let _ = req.ack.send(r);
            }
            Err(_) => {
                if let Err(e) = store.flush() {
                    error!("raft persist thread {node_id}: final flush error: {e}");
                }
                break;
            }
        }
    })?;
    Ok(PersistHandle { tx, join })
}

pub(crate) struct RaftNode {
    core: RaftCore,
    in_rx: SyncReceiver<RaftIn>,
    transport: Arc<dyn RaftTransport>,
    persist_tx: Option<ChannelSender<PersistReq>>,
    persist_join: Option<SJoinHandle<()>>,
    pending: HashMap<u64, (u64, SyncSender<ProposeResult>)>,
    pending_reconf: Option<(ConfVersion, SyncSender<UpdateConfResult>)>,
    commit_watchers: Vec<ChannelSender<(u64, ReplEntry)>>,
    status_watchers: Vec<ChannelSender<NodeStatus>>,
    status_shared: Arc<SRwLock<NodeStatus>>,
    last_status: NodeStatus,
    tick_dur: Duration,
    manual_ticks: bool,
}

impl RaftNode {
    pub(crate) fn new(
        core: RaftCore,
        in_rx: SyncReceiver<RaftIn>,
        transport: Arc<dyn RaftTransport>,
        persist: PersistHandle,
        status_shared: Arc<SRwLock<NodeStatus>>,
        tick_dur: Duration,
        manual_ticks: bool,
    ) -> RS<Self> {
        let last_status = status_shared.read()?.clone();
        Ok(Self {
            core,
            in_rx,
            transport,
            persist_tx: Some(persist.tx),
            persist_join: Some(persist.join),
            pending: HashMap::new(),
            pending_reconf: None,
            commit_watchers: vec![],
            status_watchers: vec![],
            status_shared,
            last_status,
            tick_dur,
            manual_ticks,
        })
    }

    pub(crate) fn run(mut self) -> RS<()> {
        let r = self.run_loop();
        self.shutdown();
        r
    }

    fn run_loop(&mut self) -> RS<()> {
        loop {
            let next = if self.manual_ticks {
                self.in_rx.recv().map(Some)
            } else {
                self.in_rx.recv_timeout(self.tick_dur)
            };
            let mut fx = Effects::default();
            // Responses of snapshot/compact requests are deferred until
            // after flush_effects so persistence precedes visibility.
            let mut deferred: Option<DeferredResp> = None;
            // The reconf acknowledgement is likewise deferred until the
            // new configuration is durable.
            let mut deferred_reconf: Option<DeferredReConf> = None;
            if !self.handle_input(next, &mut fx, &mut deferred, &mut deferred_reconf)? {
                break;
            }
            let flush_r = self.flush_effects(fx);
            // Deferred responses must be sent even when persistence fails
            // (a caller must never hang); the loop exits on flush errors
            // only after every deferred response has been delivered.
            let mut flush_failed = flush_r.err();
            self.send_deferred(deferred.take(), &mut flush_failed)?;
            self.send_deferred_reconf(deferred_reconf.take(), &mut flush_failed)?;
            if let Some(fe) = flush_failed {
                return Err(fe);
            }
            self.check_reconf_completion();
            self.check_leadership();
        }
        Ok(())
    }

    /// Handles one input message (or one tick on receive timeout),
    /// filling `fx` and the deferred response slots. Returns `Ok(false)`
    /// when the loop must stop (a Stop message or a closed input
    /// channel).
    fn handle_input(
        &mut self,
        next: RS<Option<RaftIn>>,
        fx: &mut Effects,
        deferred: &mut Option<DeferredResp>,
        deferred_reconf: &mut Option<DeferredReConf>,
    ) -> RS<bool> {
        match next {
            Ok(Some(RaftIn::Net { from, msg })) => {
                self.core.step_incoming(from, msg, fx)?;
            }
            Ok(Some(RaftIn::Propose { entry, resp })) => match self.core.step_propose(entry, fx) {
                ProposeOutcome::Accepted { index, term } => {
                    self.pending.insert(index, (term, resp));
                }
                ProposeOutcome::NotLeader { leader_hint } => {
                    let _ = resp.send(Err(ProposeError::not_leader(leader_hint)));
                }
            },
            Ok(Some(RaftIn::WatchCommits { tx })) => {
                self.commit_watchers.push(tx);
            }
            Ok(Some(RaftIn::WatchStatus { tx })) => {
                let _ = tx.send(self.last_status.clone());
                self.status_watchers.push(tx);
            }
            Ok(Some(RaftIn::InstallSnapshot { frontier, resp })) => {
                let r = self.core.step_install_snapshot(frontier, fx);
                *deferred = Some((resp, r, Some(frontier.index)));
            }
            Ok(Some(RaftIn::ForceCompact { resp })) => {
                let r = self.core.step_force_compact(fx);
                *deferred = Some((resp, r, None));
            }
            Ok(Some(RaftIn::UpdateConf {
                conf_value,
                nid_vote,
                nid_log,
                ack,
                done,
            })) => {
                let r = self
                    .core
                    .step_re_conf_begin(conf_value, nid_vote, nid_log, fx);
                *deferred_reconf = Some((ack, done, r));
            }
            Ok(Some(RaftIn::QueryConf { resp })) => {
                let committed = self.core.raft_conf().conf_node_value_committed().clone();
                let new = self.core.raft_conf().conf_node_value_new().clone();
                let _ = resp.send((committed, new));
            }
            Ok(Some(RaftIn::Tick)) => {
                self.core.step_tick(fx)?;
            }
            Ok(Some(RaftIn::Stop)) => {
                debug!("raft node {} stopping", self.core.node_id());
                return Ok(false);
            }
            Ok(None) => {
                // recv_timeout expired: one tick
                self.core.step_tick(fx)?;
            }
            Err(_) => {
                // input channel closed
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Delivers a deferred snapshot/compact response after the effects
    /// of its step are durable; on a flush failure the response carries
    /// the failure and the flush error is returned once the response
    /// has been delivered.
    fn send_deferred(
        &mut self,
        deferred: Option<DeferredResp>,
        flush_failed: &mut Option<MuduError>,
    ) -> RS<()> {
        if let Some((resp, step_r, complete_through)) = deferred {
            match (step_r, flush_failed.take()) {
                (Ok(()), None) => {
                    if let Some(index) = complete_through {
                        self.complete_pending_through(index);
                    }
                    let _ = resp.send(Ok(()));
                }
                (Err(e), flush_e) => {
                    let _ = resp.send(Err(e));
                    if let Some(fe) = flush_e {
                        return Err(fe);
                    }
                }
                (Ok(()), Some(fe)) => {
                    let _ = resp.send(Err(mudu_error!(fe.ec(), "raft persistence failed")));
                    return Err(fe);
                }
            }
        }
        Ok(())
    }

    /// Delivers a deferred re-configuration acknowledgement: on
    /// acceptance the completion channel is parked in `pending_reconf`
    /// until the committed configuration reaches the stamped version; a
    /// flush failure rejects the acknowledgement and is returned once
    /// it has been delivered.
    fn send_deferred_reconf(
        &mut self,
        deferred_reconf: Option<DeferredReConf>,
        flush_failed: &mut Option<MuduError>,
    ) -> RS<()> {
        if let Some((ack, done, outcome)) = deferred_reconf {
            match (outcome, flush_failed.take()) {
                (Ok(ReConfOutcome::Accepted { version }), None) => {
                    self.pending_reconf = Some((version.clone(), done));
                    let _ = ack.send(Ok(version));
                }
                (Ok(ReConfOutcome::Accepted { .. }), Some(fe)) => {
                    let _ = ack.send(Err(UpdateConfError::from_error(mudu_error!(
                        fe.ec(),
                        "raft persistence failed"
                    ))));
                    return Err(fe);
                }
                (Ok(ReConfOutcome::NotLeader { leader_hint }), _) => {
                    let _ = ack.send(Err(UpdateConfError::not_leader(leader_hint)));
                }
                (Ok(ReConfOutcome::ReconfigInProgress), _) => {
                    let _ = ack.send(Err(UpdateConfError::reconfig_in_progress()));
                }
                (Err(e), _) => {
                    let _ = ack.send(Err(UpdateConfError::from_error(e)));
                }
            }
        }
        Ok(())
    }

    /// Persists writes (waiting for durability), sends messages, then
    /// delivers committed entries and status updates.
    fn flush_effects(&mut self, mut fx: Effects) -> RS<()> {
        if !fx.writes.is_empty() {
            let persist_tx = self.persist_tx.as_ref().ok_or_else(|| {
                mudu_error!(ErrorCode::InvalidState, "raft persist thread is gone")
            })?;
            let (ack_tx, ack_rx) = sync_channel(1);
            persist_tx.send(PersistReq {
                ops: std::mem::take(&mut fx.writes),
                ack: ack_tx,
            })?;
            // wait for the batch to be durable before sending any message
            ack_rx.recv()??;
        }
        for (to, msg) in std::mem::take(&mut fx.messages) {
            if let Err(e) = self.transport.send(to, msg) {
                trace!(
                    "raft node {} send to {} error: {}",
                    self.core.node_id(),
                    to,
                    e
                );
            }
        }
        for (index, entry) in std::mem::take(&mut fx.committed) {
            if let Some((term, resp)) = self.pending.remove(&index) {
                let _ = resp.send(Ok((index, term)));
            }
            self.commit_watchers
                .retain(|tx| tx.send((index, entry.clone())).is_ok());
        }
        self.update_status()?;
        Ok(())
    }

    /// Completes pending proposals at or below `index` as committed.
    ///
    /// Used after a snapshot install: entries at or below the snapshot
    /// frontier are committed by definition (the installed snapshot covers
    /// them) but are deliberately not re-emitted as committed effects.
    fn complete_pending_through(&mut self, index: u64) {
        let done: Vec<u64> = self
            .pending
            .keys()
            .filter(|i| **i <= index)
            .cloned()
            .collect();
        for i in done {
            if let Some((term, resp)) = self.pending.remove(&i) {
                let _ = resp.send(Ok((i, term)));
            }
        }
    }

    /// Resolves a pending re-configuration waiter once the committed
    /// configuration reaches the tracked version.
    fn check_reconf_completion(&mut self) {
        if let Some((version, _)) = &self.pending_reconf {
            let committed = self
                .core
                .raft_conf()
                .conf_node_value_committed()
                .term_version()
                .clone();
            if committed >= *version {
                let (_, done) = self.pending_reconf.take().unwrap();
                let _ = done.send(Ok(committed));
            }
        }
    }

    /// Fails all pending proposals and any pending re-configuration when
    /// leadership is lost.
    fn check_leadership(&mut self) {
        if self.core.role() == crate::core::role::RaftRole::Leader {
            return;
        }
        let hint = self.core.leader_id();
        if !self.pending.is_empty() {
            warn!(
                "raft node {} lost leadership, failing {} pending proposals",
                self.core.node_id(),
                self.pending.len()
            );
            for (_, (_, resp)) in self.pending.drain() {
                let _ = resp.send(Err(ProposeError::not_leader(hint)));
            }
        }
        if let Some((_, done)) = self.pending_reconf.take() {
            warn!(
                "raft node {} lost leadership with a re-configuration in progress",
                self.core.node_id()
            );
            let _ = done.send(Err(UpdateConfError::leadership_lost(hint)));
        }
    }

    fn current_status(&self) -> NodeStatus {
        NodeStatus {
            node_id: self.core.node_id(),
            role: self.core.role(),
            term: self.core.term(),
            leader_id: self.core.leader_id(),
            commit_index: self.core.commit_index(),
            conf_version_committed: self
                .core
                .raft_conf()
                .conf_node_value_committed()
                .term_version()
                .clone(),
            conf_version_new: self
                .core
                .raft_conf()
                .conf_node_value_new()
                .term_version()
                .clone(),
        }
    }

    fn update_status(&mut self) -> RS<()> {
        let status = self.current_status();
        if status != self.last_status {
            self.last_status = status.clone();
            *self.status_shared.write()? = status.clone();
            self.status_watchers
                .retain(|tx| tx.send(status.clone()).is_ok());
        }
        Ok(())
    }

    fn shutdown(&mut self) {
        for (_, (_, resp)) in self.pending.drain() {
            let _ = resp.send(Err(ProposeError::stopped()));
        }
        if let Some((_, done)) = self.pending_reconf.take() {
            let _ = done.send(Err(UpdateConfError::stopped()));
        }
        // close the persist channel; the persist thread drains queued
        // batches and exits
        drop(self.persist_tx.take());
        if let Some(join) = self.persist_join.take() {
            if join.join().is_err() {
                error!("raft node {} persist thread panicked", self.core.node_id());
            }
        }
    }
}
