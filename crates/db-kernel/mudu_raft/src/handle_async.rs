//! Nonblocking administration and subscriptions of a Raft driver.

use crate::completion::completion;
use crate::inbox::RaftIn;
use crate::{
    ConfNodeValue, ConfValue, NodeStatus, RaftHandle, ReplEntry, TermIndex, UpdateConfError,
    UpdateConfWaiter,
};
use mudu::common::result::RS;
use mudu_sys::tokio::sync::{mpsc, watch};

impl RaftHandle {
    /// Reads the locally durable committed log after `after`, capped at 256
    /// entries. A cursor below the returned snapshot frontier requires a service
    /// snapshot. This method alone does not establish linearizable authority.
    pub async fn read_committed_async(
        &self,
        after: u64,
        limit: usize,
    ) -> RS<crate::replay::CommittedBatch> {
        if limit == 0 {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidArgument,
                "committed log page size must be positive"
            ));
        }
        let (resp, mut wait) = completion();
        self.in_tx
            .send(RaftIn::ReadCommitted { after, limit, resp })?;
        wait.wait().await
    }

    /// Proposes only if the exact membership pair used for payload durability
    /// still applies when the serialized protocol owner accepts the request.
    pub fn propose_under_membership(
        &self,
        entry: ReplEntry,
        expected_conf: crate::stream::MembershipVersion,
    ) -> RS<crate::RaftProposeWaiter> {
        self.propose_in_term(entry, expected_conf, self.term())
    }

    /// Admits a proposal only under the supplied membership and leader term.
    /// This also fences opaque management entries without a payload term.
    pub fn propose_in_term(
        &self,
        entry: ReplEntry,
        expected_conf: crate::stream::MembershipVersion,
        expected_term: u64,
    ) -> RS<crate::RaftProposeWaiter> {
        self.propose_guarded(entry, expected_conf, expected_term, Vec::new())
    }

    /// Checks membership, term and up to 256 revocable certificates at admission.
    pub fn propose_guarded(
        &self,
        entry: ReplEntry,
        expected_conf: crate::stream::MembershipVersion,
        expected_term: u64,
        guards: Vec<crate::admission::AdmissionGuard>,
    ) -> RS<crate::RaftProposeWaiter> {
        if guards.len() > 256 {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidArgument,
                "too many proposal admission certificates"
            ));
        }
        let (resp, rx) = completion();
        self.in_tx.send(RaftIn::Propose {
            entry,
            resp,
            expected_conf: Some(expected_conf),
            expected_term: Some(expected_term),
            guards,
        })?;
        Ok(crate::RaftProposeWaiter { rx })
    }

    /// Subscribes to ordered commits. Saturation stops the driver rather than
    /// skipping an entry; consumers must recover before serving again.
    pub fn watch_committed_async(&self) -> RS<mpsc::Receiver<(u64, ReplEntry)>> {
        let (tx, rx) = mpsc::channel(1024);
        self.in_tx.send(RaftIn::WatchCommitsAsync { tx })?;
        Ok(rx)
    }

    /// Subscribes to the latest durable status. Intermediate states can coalesce;
    /// consumers must also fence on term changes, not just role transitions.
    pub fn watch_status_async(&self) -> RS<watch::Receiver<NodeStatus>> {
        let (tx, rx) = watch::channel(self.status());
        self.in_tx.send(RaftIn::WatchStatusAsync { tx })?;
        Ok(rx)
    }

    /// Reads the durable configuration without blocking the caller's executor.
    pub async fn conf_async(&self) -> RS<(ConfNodeValue, ConfNodeValue)> {
        let (resp, mut wait) = completion();
        self.in_tx.send(RaftIn::QueryConf { resp })?;
        wait.wait().await
    }

    /// Installs an external snapshot frontier and waits for durability asynchronously.
    pub async fn install_snapshot_async(&self, frontier: TermIndex) -> RS<()> {
        let (resp, mut wait) = completion();
        self.in_tx
            .send(RaftIn::InstallSnapshot { frontier, resp })?;
        wait.wait().await?
    }

    /// Forces compaction and waits for its durable boundary asynchronously.
    pub async fn force_compact_async(&self) -> RS<()> {
        let (resp, mut wait) = completion();
        self.in_tx.send(RaftIn::ForceCompact { resp })?;
        wait.wait().await?
    }

    /// Begins a configuration change and asynchronously waits for local durability.
    pub async fn update_conf_async(
        &self,
        conf_value: ConfValue,
        nid_vote: Vec<u64>,
        nid_log: Vec<u64>,
    ) -> Result<UpdateConfWaiter, UpdateConfError> {
        self.update_conf_fenced_async(conf_value, nid_vote, nid_log, None)
            .await
    }

    /// Begins a change only if external stream catch-up still covers the exact
    /// log prefix and configuration at protocol admission.
    pub async fn update_conf_fenced_async(
        &self,
        conf_value: ConfValue,
        nid_vote: Vec<u64>,
        nid_log: Vec<u64>,
        fence: Option<crate::admission::MembershipFence>,
    ) -> Result<UpdateConfWaiter, UpdateConfError> {
        let (ack, mut ready) = completion();
        let (done, rx) = completion();
        self.in_tx
            .send(RaftIn::UpdateConf {
                conf_value,
                nid_vote,
                nid_log,
                fence,
                ack,
                done,
            })
            .map_err(UpdateConfError::from_error)?;
        let version = ready.wait().await.map_err(UpdateConfError::from_error)??;
        Ok(UpdateConfWaiter { version, rx })
    }

    /// Drains in-flight IO before completing shutdown. Cancelling this wait does
    /// not cancel the driver or its outstanding filesystem operations.
    /// Idempotent: a shared group may be stopped by more than one owner, and an
    /// already-exited driver returns its recorded result.
    pub async fn stop_async(&self) -> RS<()> {
        let mut stopped = self.stopped.clone();
        if let Some(result) = stopped.borrow_and_update().clone() {
            return result;
        }
        if self.in_tx.send(RaftIn::Stop).is_err() {
            // The driver already exited (e.g. a concurrent stop): read the
            // result it recorded, and fail closed only if none exists.
            if let Some(result) = stopped.borrow_and_update().clone() {
                return result;
            }
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::ChannelClosed,
                "raft driver exited without a result"
            ));
        }
        loop {
            if let Some(result) = stopped.borrow_and_update().clone() {
                return result;
            }
            stopped.changed().await.map_err(|_| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::ChannelClosed,
                    "raft driver stopped without a result"
                )
            })?;
        }
    }
}
