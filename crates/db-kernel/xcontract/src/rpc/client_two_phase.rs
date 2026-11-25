//! Remote two-phase-commit participant calls of the partition RPC
//! client: prepare,
//! commit, abort, and the legacy fire-and-forget cross-partition apply.

use super::*;

impl WorkerXContract {
    pub(super) async fn remote_apply_cross_partition_tx(
        &self,
        target_worker_id: OID,
        tx_id: OID,
        partition_id: OID,
        visibility_epoch: u64,
        partition_write_set: Vec<XLWrite>,
    ) -> RS<()> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::ApplyCrossPartitionTx {
                    tx_id,
                    coordinator_worker_id: self.worker_id,
                    partition_id,
                    visibility_epoch,
                    partition_write_set,
                },
            )
            .await?
        {
            PartitionRpcResponse::ApplyCrossPartitionTx => Ok(()),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected apply_cross_partition_tx rpc response"
            )),
        }
    }

    /// Phase 1 of two-phase commit on a remote participant.
    pub(crate) async fn remote_prepare_tx(
        &self,
        target_worker_id: OID,
        coordinator: &catalog::contract::tx_decision::CoordinatorRef,
        partition_id: OID,
        write_set: Vec<XLWrite>,
        snapshot_epoch: u64,
    ) -> RS<Option<u64>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::PrepareTx {
                    coordinator: coordinator.clone(),
                    target: self.participant_target(coordinator, partition_id).await?,
                    partition_id,
                    write_set,
                    snapshot_epoch,
                },
            )
            .await?
        {
            PartitionRpcResponse::PrepareVote {
                proposed_epoch,
                yes,
            } => Ok(yes.then_some(proposed_epoch)),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected prepare_tx rpc response"
            )),
        }
    }

    /// Phase 2 (commit) on a remote participant.
    pub(crate) async fn remote_commit_tx(
        &self,
        target_worker_id: OID,
        coordinator: &catalog::contract::tx_decision::CoordinatorRef,
        partition_id: OID,
        commit_epoch: u64,
    ) -> RS<()> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::CommitTx {
                    coordinator: coordinator.clone(),
                    target: self.participant_target(coordinator, partition_id).await?,
                    partition_id,
                    commit_epoch,
                },
            )
            .await?
        {
            PartitionRpcResponse::TxFinished => Ok(()),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected commit_tx rpc response"
            )),
        }
    }

    /// Phase 2 (abort) on a remote participant.
    pub(crate) async fn remote_abort_tx(
        &self,
        target_worker_id: OID,
        coordinator: &catalog::contract::tx_decision::CoordinatorRef,
        partition_id: OID,
    ) -> RS<()> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::AbortTx {
                    coordinator: coordinator.clone(),
                    target: self.participant_target(coordinator, partition_id).await?,
                    partition_id,
                },
            )
            .await?
        {
            PartitionRpcResponse::TxFinished => Ok(()),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected abort_tx rpc response"
            )),
        }
    }
}
