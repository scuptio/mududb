//! Resolve decisions by stable coordinator group, never by shard placement.
use super::*;
use crate::rpc::coordinator_client::tx_decision_client;
use catalog::contract::tx_decision::CoordinatorRef;
use catalog::contract::tx_decision_state::CoordinatorState;

impl WorkerXContract {
    pub(super) async fn participant_target(
        &self,
        coordinator: &CoordinatorRef,
        partition: OID,
    ) -> RS<catalog::contract::tx_decision::TxParticipant> {
        self.query_coordinator(coordinator)
            .await?
            .and_then(|state| {
                state
                    .participants
                    .into_iter()
                    .find(|participant| participant.partition_id == partition)
            })
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "coordinator participant destination is unknown"
                )
            })
    }

    pub(super) async fn query_coordinator(
        &self,
        coordinator: &CoordinatorRef,
    ) -> RS<Option<CoordinatorState>> {
        let client = tx_decision_client(&self.tx_decision_client)?;
        let local = client.coordinator_ref(coordinator.tx_id)?;
        if local == *coordinator {
            return client.transaction_state(coordinator).await;
        }
        if local.cluster_id != coordinator.cluster_id {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "foreign coordinator cluster"
            ));
        }
        let router = self
            .cluster_router()
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "coordinator directory missing"))?;
        let group = coordinator.group_id;
        if router.group_generation(group)? != coordinator.group_generation {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "coordinator generation mismatch"
            ));
        }
        match self
            .send_partition_rpc(
                router.resolve_group_worker(group, 0)?,
                PartitionRpcRequest::QueryTxDecision {
                    coordinator: coordinator.clone(),
                },
            )
            .await?
        {
            PartitionRpcResponse::TxDecisionStatus(state) => Ok(state),
            PartitionRpcResponse::Err(message) => Err(mudu_error!(ErrorCode::Transaction, message)),
            _ => Err(mudu_error!(
                ErrorCode::InvalidState,
                "unexpected coordinator query response"
            )),
        }
    }
}
