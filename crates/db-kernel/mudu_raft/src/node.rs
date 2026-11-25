//! Serialized asynchronous Raft driver. The core stays synchronous; every
//! write batch is awaited before its outbound messages or results are released.
//! At most one batch is in flight, and shutdown never cancels disk operations.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::completion::CompletionSender;
use crate::inbox::{RaftIn, RaftInbox};
use mudu::common::result::RS;
use mudu::error::{ErrorCode, MuduError};
use mudu::mudu_error;
use mudu_sys::sync::SRwLock;
use mudu_sys::tokio;
use tracing::{debug, trace, warn};

use crate::api::{NodeStatus, ProposeError, ProposeResult, UpdateConfError, UpdateConfResult};
use crate::core::conf::ConfVersion;
use crate::core::{Effects, ProposeOutcome, RaftCore, ReConfOutcome};
use crate::entry::ReplEntry;
use crate::store::AsyncStore;
use crate::transport::RaftTransport;

#[cfg(test)]
#[path = "node_tests.rs"]
mod tests;

/// A deferred request response: sent only after the effects of the same
/// step are durable. Carries the response channel, the step result, and an
/// optional log index through which pending proposals must be completed.
type DeferredResp = (CompletionSender<RS<()>>, RS<()>, Option<u64>);

/// A deferred re-configuration acknowledgement: the accept/reject ack is
/// sent only after the new configuration is durable; on acceptance the
/// completion channel is parked in `pending_reconf` until the committed
/// configuration reaches the stamped version.
type DeferredReConf = (
    CompletionSender<Result<ConfVersion, UpdateConfError>>,
    CompletionSender<UpdateConfResult>,
    RS<ReConfOutcome>,
);

/// Serialized asynchronous driver of one raft node.
pub struct RaftNode {
    core: RaftCore,
    in_rx: RaftInbox,
    transport: Arc<dyn RaftTransport>,
    store: Box<dyn AsyncStore>,
    pending: HashMap<u64, (u64, CompletionSender<ProposeResult>)>,
    pending_reconf: Option<(ConfVersion, CompletionSender<UpdateConfResult>)>,
    async_commits: Vec<tokio::sync::mpsc::Sender<(u64, ReplEntry)>>,
    async_status: Vec<tokio::sync::watch::Sender<NodeStatus>>,
    status_shared: Arc<SRwLock<NodeStatus>>,
    last_status: NodeStatus,
    tick_dur: Duration,
    manual_ticks: bool,
}

impl RaftNode {
    /// Constructs a driver from the recovered core, ingress, and store.
    pub fn new(
        core: RaftCore,
        in_rx: RaftInbox,
        transport: Arc<dyn RaftTransport>,
        store: Box<dyn AsyncStore>,
        status_shared: Arc<SRwLock<NodeStatus>>,
        tick_dur: Duration,
        manual_ticks: bool,
    ) -> RS<Self> {
        let last_status = status_shared.read()?.clone();
        Ok(Self {
            core,
            in_rx,
            transport,
            store,
            pending: HashMap::new(),
            pending_reconf: None,
            async_commits: vec![],
            async_status: vec![],
            status_shared,
            last_status,
            tick_dur,
            manual_ticks,
        })
    }

    /// Runs the serialized event loop until the node stops.
    pub async fn run(mut self) -> RS<()> {
        let r = self.run_loop().await;
        self.shutdown();
        r?;
        self.store.flush().await
    }

    async fn run_loop(&mut self) -> RS<()> {
        let mut ticks = tokio::time::interval(self.tick_dur.max(Duration::from_millis(1)));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticks.tick().await;
        let mut budget = 0;
        loop {
            let next = tokio::select! {
                biased;
                _ = ticks.tick(), if !self.manual_ticks => Ok(None),
                event = self.in_rx.recv() => match event {
                    Some(event) => Ok(Some(event)),
                    None => break,
                },
            };
            let mut fx = Effects::default();
            let mut deferred = None;
            let mut deferred_reconf = None;
            if !self.handle_input(next, &mut fx, &mut deferred, &mut deferred_reconf)? {
                break;
            }
            let mut flush_failed = self.flush_effects(fx).await.err();
            self.send_deferred(deferred, &mut flush_failed)?;
            self.send_deferred_reconf(deferred_reconf, &mut flush_failed)?;
            if let Some(error) = flush_failed {
                return Err(error);
            }
            self.check_reconf_completion();
            self.check_leadership();
            budget += 1;
            if budget == 32 {
                tokio::task::yield_now().await;
                budget = 0;
            }
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
            Ok(Some(RaftIn::ReadCommitted { after, limit, resp })) => {
                let _ = resp.send(self.core.read_committed(after, limit));
            }
            Ok(Some(RaftIn::Net { from, msg })) => {
                self.core.step_incoming(from, msg, fx)?;
            }
            Ok(Some(RaftIn::Propose {
                entry,
                resp,
                expected_conf,
                expected_term,
                guards,
            })) => {
                self.handle_proposal(entry, resp, expected_conf, expected_term, guards, fx);
            }
            Ok(Some(RaftIn::WatchCommitsAsync { tx })) => self.async_commits.push(tx),
            Ok(Some(RaftIn::WatchStatusAsync { tx })) => {
                tx.send_replace(self.last_status.clone());
                self.async_status.push(tx);
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
                fence,
                ack,
                done,
            })) => {
                let valid = fence.as_ref().is_none_or(|proof| {
                    proof.term == self.core.term()
                        && proof.committed_index == self.core.commit_index()
                        && proof.committed_index == self.core.last_log_index()
                        && proof.membership.matches(
                            self.core.raft_conf().conf_node_value_committed(),
                            self.core.raft_conf().conf_node_value_new(),
                        )
                        && proof.guards.len() <= 256
                        && proof
                            .guards
                            .iter()
                            .all(crate::admission::AdmissionGuard::is_valid)
                });
                let r = if valid {
                    self.core
                        .step_re_conf_begin(conf_value, nid_vote, nid_log, fx)
                } else {
                    Err(mudu_error!(
                        ErrorCode::ResourceBusy,
                        "membership catch-up proof is stale"
                    ))
                };
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

    fn handle_proposal(
        &mut self,
        entry: ReplEntry,
        resp: CompletionSender<ProposeResult>,
        expected_conf: Option<crate::stream::MembershipVersion>,
        expected_term: Option<u64>,
        guards: Vec<crate::admission::AdmissionGuard>,
        fx: &mut Effects,
    ) {
        let invalid_proof = expected_conf.is_some_and(|version| {
            !version.matches(
                self.core.raft_conf().conf_node_value_committed(),
                self.core.raft_conf().conf_node_value_new(),
            ) || entry
                .payload_term()
                .is_some_and(|term| term != self.core.term())
        });
        if self.pending.len() >= 1024
            || invalid_proof
            || expected_term.is_some_and(|term| term != self.core.term())
            || guards.len() > 256
            || guards.iter().any(|guard| !guard.is_valid())
        {
            let _ = resp.send(Err(ProposeError::from_error(mudu_error!(
                ErrorCode::ResourceBusy,
                "raft pending limit reached or payload proof is stale"
            ))));
            return;
        }
        match self.core.step_propose(entry, fx) {
            ProposeOutcome::Accepted { index, term } => {
                self.pending.insert(index, (term, resp));
            }
            ProposeOutcome::NotLeader { leader_hint } => {
                let _ = resp.send(Err(ProposeError::not_leader(leader_hint)));
            }
        }
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
    /// publishes status before delivering committed entries.
    async fn flush_effects(&mut self, mut fx: Effects) -> RS<()> {
        if !fx.writes.is_empty() {
            self.store.apply_batch(&fx.writes).await?;
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
        // Publish the durable state before waking any proposal waiter or
        // commit watcher: callers may immediately read status after a
        // notification and must not observe the previous commit index.
        self.update_status()?;
        for (index, entry) in std::mem::take(&mut fx.committed) {
            if let Some((term, resp)) = self.pending.remove(&index) {
                let _ = resp.send(Ok((index, term)));
            }
            let mut saturated = false;
            self.async_commits
                .retain(|tx| match tx.try_send((index, entry.clone())) {
                    Ok(()) => true,
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => false,
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        saturated = true;
                        false
                    }
                });
            if saturated {
                return Err(mudu_error!(
                    ErrorCode::ResourceBusy,
                    "raft commit subscriber lagged; recovery required"
                ));
            }
        }
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
            self.async_status
                .retain(|tx| tx.send(status.clone()).is_ok());
        }
        Ok(())
    }

    fn shutdown(&mut self) {
        // Fail closed even on disk or subscriber failure. A stale leader
        // status must never keep the data-plane serving gate open.
        let mut stopped = self.last_status.clone();
        stopped.role = crate::core::role::RaftRole::Follower;
        stopped.leader_id = None;
        if let Ok(mut status) = self.status_shared.write() {
            *status = stopped.clone();
        }
        for tx in &self.async_status {
            let _ = tx.send(stopped.clone());
        }
        for (_, (_, resp)) in self.pending.drain() {
            let _ = resp.send(Err(ProposeError::stopped()));
        }
        if let Some((_, done)) = self.pending_reconf.take() {
            let _ = done.send(Err(UpdateConfError::stopped()));
        }
        self.in_rx.close();
    }
}
