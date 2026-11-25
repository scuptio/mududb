//! Authoritative decision commands and linearizable coordinator queries.
use super::machine::TX_DECISION_STREAM;
use super::service::DataPlaneService;
use crate::log_stream::StreamStateMachine;
use catalog::contract::tx_decision::{
    CoordinatorRef, TxDecision, TxDecisionStore, TxDecisionStruct, TxDecisionSubmitter,
};
use catalog::contract::tx_decision_state::CoordinatorState;
use mudu::common::{id::OID, result::RS};
use mudu::{error::ErrorCode, mudu_error};
use std::time::Duration;

impl DataPlaneService {
    /// Persists a local checkpoint of the applied decision stream, including
    /// unresolved transactions and retained command results. The returned
    /// prefix is not a group-wide snapshot or permission to reclaim logs.
    /// Requires every stream to be applied at the sampled committed vector and
    /// holds all apply owners during capture; lagging/ahead streams require retry.
    pub async fn checkpoint_tx_decisions(&self) -> RS<Option<mudu_raft::stream::StreamProgress>> {
        let scheduler = self.scheduler.lock()?.clone().ok_or_else(|| {
            mudu_error!(ErrorCode::InvalidState, "data apply scheduler is not ready")
        })?;
        let state = self.registry.refresh().await?;
        let _cut = scheduler.capture_applied_cut(&state).await?;
        self.tx_machine.checkpoint().await
    }

    /// Binds a transaction to this durable group incarnation.
    pub fn coordinator_ref(&self, tx_id: OID) -> CoordinatorRef {
        let identity = self.logs[&TX_DECISION_STREAM].identity();
        CoordinatorRef {
            cluster_id: identity.cluster_id,
            group_id: identity.group_id.0,
            group_generation: identity.generation,
            tx_id,
        }
    }

    /// Returns the authoritative transaction lifecycle after a leader barrier
    /// and local apply through the barrier's committed decision-stream prefix.
    /// Unknown remains None and is never interpreted as an abort.
    pub async fn transaction_state(
        &self,
        coordinator: &CoordinatorRef,
    ) -> RS<Option<CoordinatorState>> {
        self.validate_coordinator(coordinator)?;
        let query = async {
            let (_, state) = self.registry.leader_view().await?;
            if let Some(progress) = state.committed().get(&TX_DECISION_STREAM) {
                self.wait_decision_apply(progress.end_lsn).await?;
            }
            self.tx_machine.state(coordinator)
        };
        mudu_sys::timeout(Duration::from_secs(30), query)
            .await
            .ok_or_else(|| mudu_error!(ErrorCode::TimedOut, "decision query timed out"))?
    }

    /// Submits a lifecycle command; a committed business rejection is returned
    /// as an error without stopping subsequent commands in this stream.
    pub async fn submit_tx_record(&self, request_id: u128, record: TxDecisionStruct) -> RS<()> {
        self.validate_coordinator(record.coordinator())?;
        let command = async {
            let payload = crate::log_stream::frontier::encode(&record)?;
            let receipt = self
                .group
                .append(TX_DECISION_STREAM, request_id, payload, vec![])
                .await?;
            self.wait_decision_apply(receipt.progress.end_lsn).await?;
            self.tx_machine
                .result(receipt.progress.end_lsn)?
                .ok_or_else(|| {
                    mudu_error!(ErrorCode::InvalidState, "applied decision result missing")
                })?
                .map_err(|error| {
                    mudu_error!(
                        ErrorCode::Transaction,
                        format!("decision command rejected: {error:?}")
                    )
                })
        };
        mudu_sys::timeout(Duration::from_secs(30), command)
            .await
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::TimedOut,
                    "decision command timed out; retry the same request id"
                )
            })?
    }

    pub(super) async fn wait_decision_apply(&self, through: u64) -> RS<()> {
        let mut stop = self.stopping.subscribe();
        loop {
            let changed = self.tx_machine.changed().notified();
            if *stop.borrow() {
                return Err(mudu_error!(
                    ErrorCode::ChannelClosed,
                    "decision service stopped"
                ));
            }
            if self
                .tx_machine
                .applied_frontier()
                .await?
                .is_some_and(|p| p.end_lsn >= through)
            {
                return Ok(());
            }
            tokio::select! {
                _ = changed => {},
                _ = stop.changed() => {},
            }
        }
    }

    /// Captures unresolved coordinator work only after a fresh leadership barrier.
    pub async fn pending_transactions(&self) -> RS<(u64, Vec<CoordinatorState>)> {
        mudu_sys::timeout(Duration::from_secs(30), async {
            let (term, state) = self.registry.leader_view().await?;
            if let Some(progress) = state.committed().get(&TX_DECISION_STREAM) {
                self.wait_decision_apply(progress.end_lsn).await?;
            }
            Ok((term, self.tx_machine.pending()?))
        })
        .await
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::TimedOut,
                "coordinator recovery barrier timed out"
            )
        })?
    }

    fn validate_coordinator(&self, coordinator: &CoordinatorRef) -> RS<()> {
        if self.coordinator_ref(coordinator.tx_id) != *coordinator {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "foreign coordinator group or incarnation"
            ));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl TxDecisionStore for DataPlaneService {
    async fn decision(&self, coordinator: &CoordinatorRef) -> RS<Option<TxDecision>> {
        Ok(self
            .transaction_state(coordinator)
            .await?
            .and_then(|state| state.decision))
    }
}

#[async_trait::async_trait]
impl TxDecisionSubmitter for DataPlaneService {
    async fn submit(&self, request_id: u128, record: TxDecisionStruct) -> RS<()> {
        self.submit_tx_record(request_id, record).await
    }
}
