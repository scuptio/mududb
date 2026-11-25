//! Leader-only recovery of durable coordinator and participant work.
use super::*;
use crate::rpc::coordinator_client::tx_decision_client;
use catalog::contract::tx_decision::{TxDecision, TxDecisionClient, TxDecisionStruct};
use catalog::contract::tx_decision_state::CoordinatorState;

impl WorkerXContract {
    /// Resolve durable participant prepares and take over coordinator work
    /// from earlier terms. Unknown outcomes retain the prepare and its fence.
    pub async fn recover_pending_cross_partition_records_async(&self) -> RS<()> {
        if self
            .raft_share()?
            .is_none_or(|share| !share.is_serving_leader())
        {
            return Ok(());
        }
        let client = match tx_decision_client(&self.tx_decision_client) {
            Ok(client) => client,
            Err(_) => return Ok(()),
        };
        for prepare in self.storage.pending_participant_records()? {
            let outcome = async {
                let Some(state) = self.query_coordinator(&prepare.coordinator).await? else {
                    return Ok(());
                };
                match state.decision {
                    Some(TxDecision::Committed { commit_epoch }) => {
                        self.execute_rpc_commit_tx(
                            prepare.coordinator.clone(),
                            prepare.partition_id,
                            commit_epoch,
                        )
                        .await?;
                    }
                    Some(TxDecision::Aborted) => {
                        self.execute_rpc_abort_tx(
                            prepare.coordinator.clone(),
                            prepare.partition_id,
                        )
                        .await?;
                    }
                    None => {}
                }
                RS::Ok(())
            }
            .await;
            if let Err(error) = outcome {
                debug!("participant recovery deferred: {error}");
            }
        }
        // Only one worker drives the group's coordinator scan.
        if client.participant_ref(0)?.worker_index != 0 {
            return Ok(());
        }
        let (term, pending) = client.pending_transactions().await?;
        for state in pending {
            if state.decision.is_none()
                && state.owner_term >= term
                && client.coordinator_active(&state.coordinator)?
            {
                continue;
            }
            if let Err(error) = self.recover_coordinator(client.as_ref(), state).await {
                debug!("coordinator recovery deferred: {error}");
            }
        }
        Ok(())
    }

    async fn recover_coordinator(
        &self,
        client: &dyn TxDecisionClient,
        state: CoordinatorState,
    ) -> RS<()> {
        let decision = match state.decision {
            Some(decision) => decision,
            None => {
                client
                    .submit(
                        gen_oid(),
                        TxDecisionStruct::Decide {
                            coordinator: state.coordinator.clone(),
                            decision: TxDecision::Aborted,
                        },
                    )
                    .await?;
                TxDecision::Aborted
            }
        };
        let local = client.participant_ref(0)?;
        let identities: Vec<_> = state
            .participants
            .difference(&state.finished)
            .cloned()
            .collect();
        let mut participants = Vec::with_capacity(identities.len());
        for identity in &identities {
            let worker_id = if identity.group_id == local.group_id
                && identity.worker_index == local.worker_index
            {
                if identity.group_generation != local.group_generation {
                    return Err(mudu_error!(
                        ErrorCode::InvalidState,
                        "participant generation mismatch"
                    ));
                }
                self.worker_id
            } else {
                let router = self.cluster_router().ok_or_else(|| {
                    mudu_error!(ErrorCode::InvalidState, "recovery directory missing")
                })?;
                let group = identity.group_id;
                if router.group_generation(group)? != identity.group_generation {
                    return Err(mudu_error!(
                        ErrorCode::InvalidState,
                        "participant generation changed without authority transfer"
                    ));
                }
                router.resolve_group_worker(group, identity.worker_index)?
            };
            participants.push(CrossPartitionParticipant {
                partition_id: identity.partition_id,
                worker_id,
            });
        }
        self.settle_participants(
            client,
            &state.coordinator,
            &participants,
            &identities,
            decision,
        )
        .await
    }
}
