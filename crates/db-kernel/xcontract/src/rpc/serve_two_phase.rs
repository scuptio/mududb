//! Two-phase-commit participant handlers of the partition RPC service: prepare
//! (conflict validation + durable prepare record + staged write set), commit and
//! abort, plus the legacy fire-and-forget cross-partition apply kept for
//! the single-owner fast path.

use super::*;
use crate::rpc::coordinator_client::tx_decision_client;
use crate::worker_storage::WorkerStorage;
use ::catalog::contract::tx_decision::TxDecision;
use ::catalog::contract::worker_snapshot::WorkerSnapshotMgr;
use ::wal::participant::{ParticipantDecision, ParticipantPrepare};

fn prepare_vote(snapshot_mgr: &WorkerSnapshotMgr, snapshot_epoch: u64) -> PartitionRpcResponse {
    PartitionRpcResponse::PrepareVote {
        proposed_epoch: snapshot_mgr
            .epoch_oracle()
            .map(|oracle| oracle.current_epoch())
            .unwrap_or(snapshot_epoch)
            .max(snapshot_epoch),
        yes: true,
    }
}

/// Serve `ApplyCrossPartitionTx`: apply the coordinator's write set for
/// `partition_id` under `visibility_epoch`.
async fn execute_rpc_apply_cross_partition_tx(
    storage: &Arc<WorkerStorage>,
    worker_id: OID,
    tx_id: OID,
    partition_id: OID,
    visibility_epoch: u64,
    partition_write_set: Vec<XLWrite>,
) -> RS<PartitionRpcResponse> {
    debug!(
        worker_id,
        tx_id,
        partition_id,
        writes = partition_write_set.len(),
        "execute partition rpc apply_cross_partition_tx"
    );
    storage
        .apply_cross_partition_tx_async(tx_id, &partition_write_set, Some(visibility_epoch))
        .await?;
    Ok(PartitionRpcResponse::ApplyCrossPartitionTx)
}

impl WorkerXContract {
    /// Dispatches one transaction-control RPC of the partition service.
    pub(super) async fn execute_tx_rpc(
        &self,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        if let PartitionRpcRequest::PrepareTx {
            target,
            partition_id,
            ..
        }
        | PartitionRpcRequest::CommitTx {
            target,
            partition_id,
            ..
        }
        | PartitionRpcRequest::AbortTx {
            target,
            partition_id,
            ..
        } = &request
        {
            if tx_decision_client(&self.tx_decision_client)?.participant_ref(*partition_id)?
                != *target
            {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "participant destination or generation mismatch"
                ));
            }
        }
        match request {
            PartitionRpcRequest::ApplyCrossPartitionTx {
                tx_id,
                partition_id,
                visibility_epoch,
                partition_write_set,
                ..
            } => {
                execute_rpc_apply_cross_partition_tx(
                    &self.storage,
                    self.worker_id,
                    tx_id,
                    partition_id,
                    visibility_epoch,
                    partition_write_set,
                )
                .await
            }
            PartitionRpcRequest::PrepareTx {
                coordinator,
                partition_id,
                write_set,
                snapshot_epoch,
                ..
            } => {
                self.execute_rpc_prepare_tx(coordinator, partition_id, write_set, snapshot_epoch)
                    .await
            }
            PartitionRpcRequest::CommitTx {
                coordinator,
                partition_id,
                commit_epoch,
                ..
            } => {
                self.execute_rpc_commit_tx(coordinator, partition_id, commit_epoch)
                    .await
            }
            PartitionRpcRequest::AbortTx {
                coordinator,
                partition_id,
                ..
            } => self.execute_rpc_abort_tx(coordinator, partition_id).await,
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "not a transaction-control partition rpc"
            )),
        }
    }

    /// Serve `PrepareTx` (2PC phase 1): conflict-validate the write set
    /// against `snapshot_epoch`, write a durable prepare record to the WAL,
    /// and stage the write set for the decision. The write-set keys are
    /// already statement-locked by the coordinator's execution-time locks
    /// on this worker (inserts lock at write time), so prepare only
    /// validates.
    pub(crate) async fn execute_rpc_prepare_tx(
        &self,
        coordinator: catalog::contract::tx_decision::CoordinatorRef,
        partition_id: OID,
        write_set: Vec<XLWrite>,
        snapshot_epoch: u64,
    ) -> RS<PartitionRpcResponse> {
        let _gate = self.participant_gate.lock().await;
        let state = self.query_coordinator(&coordinator).await?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "prepare requires a durable coordinator Begin"
            )
        })?;
        let identity =
            tx_decision_client(&self.tx_decision_client)?.participant_ref(partition_id)?;
        if !state.participants.contains(&identity) || state.snapshot_epoch != snapshot_epoch {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "prepare disagrees with coordinator participants or snapshot"
            ));
        }
        if state.decision == Some(TxDecision::Aborted) {
            return Ok(PartitionRpcResponse::PrepareVote {
                proposed_epoch: 0,
                yes: false,
            });
        }
        let record = ParticipantPrepare {
            coordinator,
            partition_id,
            snapshot_epoch,
            write_set,
        };
        if self.storage.check_participant_prepare(&record)? {
            return Ok(prepare_vote(&self.snapshot_mgr, snapshot_epoch));
        }
        if state.decision.is_some() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "decision exists but this participant has no matching prepare"
            ));
        }
        self.storage.ensure_partition_not_prepared(partition_id)?;
        let write_set = &record.write_set;
        use ::index::index_key::key_tuple::KeyTuple;
        // Conflict validation: any key committed after the coordinator's
        // snapshot epoch aborts the transaction (SI).
        let validation_snapshot = WorkerSnapshot::new_with_read_epoch(snapshot_epoch);
        for write in write_set {
            let (table_id, key) = match write {
                XLWrite::Insert(insert) => (insert.table_id, &insert.key),
                XLWrite::Delete(delete) => (delete.table_id, &delete.key),
                XLWrite::Update(update) => (update.table_id, &update.key),
            };
            let relation = self
                .storage
                .get_relation_async(table_id, Some(partition_id))
                .await?;
            let key_tuple = KeyTuple::from(key.clone());
            if relation
                .has_write_conflict(&key_tuple, &validation_snapshot)
                .await?
            {
                return Ok(PartitionRpcResponse::PrepareVote {
                    proposed_epoch: 0,
                    yes: false,
                });
            }
        }
        // Cutover race fence (phase 3): re-check the migration fence with no
        // await before the registration, so no prepare registers after the
        // owner's fence went up (the move driver polls `prepared_pending`
        // before the placement flip; a post-fence registration would break
        // that quiescence). The fence check at the RPC door covers the
        // common case; this closes the door-to-register race.
        if let Some(router) = &self.cluster_router {
            if let Some((rule, partition)) =
                self.meta_mgr.find_partition_names(partition_id).await?
            {
                if matches!(
                    router.migration_state(&rule, &partition),
                    ::catalog::contract::cluster_router::MigrationState::Migrating { .. }
                ) {
                    return Ok(PartitionRpcResponse::PrepareVote {
                        proposed_epoch: 0,
                        yes: false,
                    });
                }
            }
        }
        self.persist_participant_op(TxOp::Prepare(record)).await?;
        Ok(prepare_vote(&self.snapshot_mgr, snapshot_epoch))
    }

    /// Final outcomes are immutable and acknowledged only after replicated
    /// WAL apply; a missing prepare never counts as a successful commit.
    pub(crate) async fn execute_rpc_commit_tx(
        &self,
        coordinator: catalog::contract::tx_decision::CoordinatorRef,
        partition_id: OID,
        commit_epoch: u64,
    ) -> RS<PartitionRpcResponse> {
        self.finish_participant(ParticipantDecision {
            coordinator,
            partition_id,
            decision: TxDecision::Committed { commit_epoch },
            commit_ts: next_commit_timestamp(),
        })
        .await
    }

    /// Abort also persists a tombstone when prepare has not arrived, so a
    /// delayed prepare cannot resurrect an already aborted participant.
    pub(crate) async fn execute_rpc_abort_tx(
        &self,
        coordinator: catalog::contract::tx_decision::CoordinatorRef,
        partition_id: OID,
    ) -> RS<PartitionRpcResponse> {
        self.finish_participant(ParticipantDecision {
            coordinator,
            partition_id,
            decision: TxDecision::Aborted,
            commit_ts: next_commit_timestamp(),
        })
        .await
    }

    async fn finish_participant(&self, record: ParticipantDecision) -> RS<PartitionRpcResponse> {
        let _gate = self.participant_gate.lock().await;
        let state = self
            .query_coordinator(&record.coordinator)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "coordinator outcome is Unknown; retaining prepare"
                )
            })?;
        let identity =
            tx_decision_client(&self.tx_decision_client)?.participant_ref(record.partition_id)?;
        if !state.participants.contains(&identity) || state.decision != Some(record.decision) {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "participant outcome lacks matching coordinator authority"
            ));
        }
        if !self.storage.check_participant_decision(&record)? {
            self.persist_participant_op(TxOp::ParticipantDecision(record))
                .await?;
        }
        Ok(PartitionRpcResponse::TxFinished)
    }
}
