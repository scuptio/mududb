//! Durable coordinator lifecycle belongs to the initiating data group.
use super::*;
use crate::kv::tx::worker_rollback_tx;
use crate::rpc::coordinator_client::tx_decision_client;
use crate::utils::statement_lock_token;
use ::catalog::contract::cluster_router::ClusterRouter;
use ::catalog::contract::meta_mgr::MetaMgr;
use catalog::contract::tx_decision::{
    CoordinatorRef, TxDecision, TxDecisionClient, TxDecisionStruct as R, TxParticipant,
};

/// Resolves the tx-decision identities of the cross-partition participants.
async fn coordinator_participants(
    cluster_router: Option<&Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    participants: &[CrossPartitionParticipant],
) -> RS<Vec<TxParticipant>> {
    let router = cluster_router.ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidState,
            "cross-group transaction requires a directory"
        )
    })?;
    let mut identities = Vec::with_capacity(participants.len());
    for participant in participants {
        let (rule, name) = meta_mgr
            .find_partition_names(participant.partition_id)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    "participant partition is not in the directory"
                )
            })?;
        let (group_id, worker_index) = router.placement(&rule, &name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                "participant placement is missing"
            )
        })?;
        identities.push(TxParticipant {
            group_id,
            group_generation: router.group_generation(group_id)?,
            worker_index,
            partition_id: participant.partition_id,
        });
    }
    Ok(identities)
}

impl WorkerXContract {
    pub(super) async fn commit_two_phase_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<Option<u64>> {
        let client = tx_decision_client(&self.tx_decision_client)?;
        // Independent 128-bit identity avoids collisions between workers whose
        // local xid allocators issue the same counter in one epoch.
        let coordinator = client.coordinator_ref(gen_oid())?;
        let _activity = client.track_coordinator(&coordinator)?;
        let owner_term = client.leader_term().await?;
        let snapshot_epoch = tx.snapshot().read_epoch().unwrap_or(0);
        let (participants, write_set) = crate::rpc::cross_partition::build_cross_partition_tx_ops(
            &self.cluster_router,
            &self.meta_mgr,
            self.default_unpartitioned_worker_id,
            self.worker_id,
            tx.as_ref(),
        )
        .await?;
        let identities =
            coordinator_participants(self.cluster_router(), &self.meta_mgr, &participants).await?;
        client
            .submit(
                gen_oid(),
                R::Begin {
                    owner_term,
                    coordinator: coordinator.clone(),
                    participants: identities.clone(),
                    snapshot_epoch,
                },
            )
            .await?;
        let prepared = self
            .two_phase_prepare_all(&coordinator, snapshot_epoch, &participants, &write_set)
            .await;
        let proposed = match prepared {
            Ok(Some(epochs)) => epochs,
            outcome => {
                client
                    .submit(
                        gen_oid(),
                        R::Decide {
                            coordinator: coordinator.clone(),
                            decision: TxDecision::Aborted,
                        },
                    )
                    .await?;
                // An unresolved participant keeps its durable fence even when
                // notification fails. Never release it on an Unknown query.
                let settle = self
                    .settle_participants(
                        client.as_ref(),
                        &coordinator,
                        &participants,
                        &identities,
                        TxDecision::Aborted,
                    )
                    .await;
                self.worker_abort_tx_async(tx).await?;
                settle?;
                return Err(outcome.err().unwrap_or_else(|| {
                    mudu_error!(ErrorCode::Transaction, "a participant voted no")
                }));
            }
        };
        let commit_epoch = proposed
            .into_iter()
            .chain(
                self.snapshot_mgr
                    .epoch_oracle()
                    .map(|oracle| oracle.current_epoch()),
            )
            .chain([snapshot_epoch])
            .max()
            .unwrap_or(snapshot_epoch);
        let decision = TxDecision::Committed { commit_epoch };
        client
            .submit(
                gen_oid(),
                R::Decide {
                    coordinator: coordinator.clone(),
                    decision,
                },
            )
            .await?;
        self.settle_participants(
            client.as_ref(),
            &coordinator,
            &participants,
            &identities,
            decision,
        )
        .await?;
        for owner in tx.remote_lock_owners() {
            let _ = self
                .remote_unlock_keys(owner, statement_lock_token(self.worker_id, tx.xid()))
                .await;
        }
        tx.clear_remote_lock_owners();
        worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)?;
        Ok(Some(commit_epoch))
    }

    pub(super) async fn settle_participants(
        &self,
        client: &dyn TxDecisionClient,
        coordinator: &CoordinatorRef,
        participants: &[CrossPartitionParticipant],
        identities: &[TxParticipant],
        decision: TxDecision,
    ) -> RS<()> {
        for (participant, identity) in participants.iter().zip(identities) {
            match (participant.worker_id == self.worker_id, decision) {
                (true, TxDecision::Committed { commit_epoch }) => {
                    self.execute_rpc_commit_tx(
                        coordinator.clone(),
                        participant.partition_id,
                        commit_epoch,
                    )
                    .await?;
                }
                (true, TxDecision::Aborted) => {
                    self.execute_rpc_abort_tx(coordinator.clone(), participant.partition_id)
                        .await?;
                }
                (false, TxDecision::Committed { commit_epoch }) => {
                    self.remote_commit_tx(
                        participant.worker_id,
                        coordinator,
                        participant.partition_id,
                        commit_epoch,
                    )
                    .await?;
                }
                (false, TxDecision::Aborted) => {
                    self.remote_abort_tx(
                        participant.worker_id,
                        coordinator,
                        participant.partition_id,
                    )
                    .await?;
                }
            }
            client
                .submit(
                    gen_oid(),
                    R::ParticipantDone {
                        coordinator: coordinator.clone(),
                        participant: identity.clone(),
                    },
                )
                .await?;
        }
        client
            .submit(
                gen_oid(),
                R::Complete {
                    coordinator: coordinator.clone(),
                },
            )
            .await
    }
    /// 2PC phase 1 on every participant: the proposed epochs, or `None`
    /// after aborting everywhere when a participant voted no.
    async fn two_phase_prepare_all(
        &self,
        coordinator: &catalog::contract::tx_decision::CoordinatorRef,
        snapshot_epoch: u64,
        participants: &[CrossPartitionParticipant],
        write_set: &[XLWrite],
    ) -> RS<Option<Vec<u64>>> {
        let mut proposed_epochs = vec![snapshot_epoch];
        for participant in participants {
            let part_writes = partition_write_set(write_set, participant.partition_id);
            let vote = if participant.worker_id == self.worker_id {
                match self
                    .execute_rpc_prepare_tx(
                        coordinator.clone(),
                        participant.partition_id,
                        part_writes,
                        snapshot_epoch,
                    )
                    .await?
                {
                    PartitionRpcResponse::PrepareVote {
                        proposed_epoch,
                        yes,
                    } => yes.then_some(proposed_epoch),
                    _ => None,
                }
            } else {
                self.remote_prepare_tx(
                    participant.worker_id,
                    coordinator,
                    participant.partition_id,
                    part_writes,
                    snapshot_epoch,
                )
                .await?
            };
            match vote {
                Some(proposed_epoch) => proposed_epochs.push(proposed_epoch),
                None => {
                    // A participant voted no: abort everywhere and fail the
                    // transaction.
                    return Ok(None);
                }
            }
        }
        Ok(Some(proposed_epochs))
    }
}
