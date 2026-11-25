use super::*;
use crate::kv::tx::{worker_begin_tx, worker_rollback_tx};
use crate::lifecycle_cluster::cluster_non_serving_redirect;
use crate::lifecycle_cluster::migration_fence;
use crate::lifecycle_cluster::migration_inflight_enter;
use crate::rpc::coordinator_client::tx_decision_client;
use crate::utils::{project_selected_fields, rpc_bound_as_ref};
use crate::worker_storage::WorkerStorage;
use crate::x_lock_mgr::XLockMgr;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::worker_snapshot::WorkerSnapshotMgr;
use mudu::channel::ServiceCtx;

/// Serve `ReadKey`: read the committed value of `key` on `partition_id`
/// under a fresh snapshot, projected to the `select` columns.
#[allow(clippy::too_many_arguments)]
async fn execute_rpc_read_key(
    meta_mgr: &Arc<dyn MetaMgr>,
    storage: &Arc<WorkerStorage>,
    snapshot_mgr: &WorkerSnapshotMgr,
    tx_lock: &XLockMgr,
    worker_id: OID,
    table_id: OID,
    partition_id: OID,
    key: Vec<u8>,
    select: Vec<AttrIndex>,
) -> RS<PartitionRpcResponse> {
    debug!(
        worker_id,
        table_id,
        partition_id,
        key_len = key.len(),
        select_len = select.len(),
        "execute partition rpc read_key"
    );
    let desc = meta_mgr.get_table_by_id(table_id).await?;
    let tx_mgr = worker_begin_tx(snapshot_mgr)?;
    let opt_value = storage
        .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
        .await?;
    worker_rollback_tx(tx_lock, snapshot_mgr, tx_mgr)?;
    let projected = opt_value
        .map(|value| project_selected_fields(&desc, &key, &value, &VecSelTerm::new(select)))
        .transpose()?;
    Ok(PartitionRpcResponse::ReadKey(projected))
}

/// Serve `ReadRange`: scan the committed rows of `partition_id` within the
/// `start`/`end` bounds with a fresh snapshot, projected to the `select`
/// columns.
#[allow(clippy::too_many_arguments)]
async fn execute_rpc_read_range(
    meta_mgr: &Arc<dyn MetaMgr>,
    storage: &Arc<WorkerStorage>,
    snapshot_mgr: &WorkerSnapshotMgr,
    tx_lock: &XLockMgr,
    worker_id: OID,
    table_id: OID,
    partition_id: OID,
    start: RpcBound,
    end: RpcBound,
    select: Vec<AttrIndex>,
) -> RS<PartitionRpcResponse> {
    debug!(
        worker_id,
        table_id,
        partition_id,
        select_len = select.len(),
        start = ?start,
        end = ?end,
        "execute partition rpc read_range"
    );
    let desc = meta_mgr.get_table_by_id(table_id).await?;
    let tx_mgr = worker_begin_tx(snapshot_mgr)?;
    let rows = storage
        .range_on_partition(
            table_id,
            Some(partition_id),
            (rpc_bound_as_ref(&start), rpc_bound_as_ref(&end)),
            tx_mgr.as_ref(),
        )
        .await?;
    worker_rollback_tx(tx_lock, snapshot_mgr, tx_mgr)?;
    let mut projected = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        projected.push(project_selected_fields(
            &desc,
            &key,
            &value,
            &VecSelTerm::new(select.clone()),
        )?);
    }
    Ok(PartitionRpcResponse::ReadRange(projected))
}

impl WorkerXContract {
    /// Handles one typed partition-RPC request; a replay under an
    /// already-seen `(src, request_id)` returns the cached response.
    pub(crate) async fn handle_partition_rpc_message(
        &self,
        request: PartitionRpcRequest,
        ctx: ServiceCtx,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            src = ctx.src(),
            request_id = ctx.request_id(),
            attempt = ctx.attempt(),
            "received partition rpc request"
        );
        // Cluster mode: a partition RPC must run on the serving node of this
        // node's group; a RaftFollower redirects to the Leader before any
        // execution, so a redirected request was definitively not executed
        // and the coordinator may retry it regardless of idempotency.
        // Migration-control RPCs (phase 3) are exempt: the move driver
        // addresses every member of a group (replicas adopt and drop their
        // own copies too).
        let migration_control = matches!(
            request,
            PartitionRpcRequest::PartitionSnapshotOffer { .. }
                | PartitionRpcRequest::PartitionSnapshotFetch { .. }
                | PartitionRpcRequest::PartitionAdopt { .. }
                | PartitionRpcRequest::PartitionDrop { .. }
                | PartitionRpcRequest::MigrationFenceQuery { .. }
        );
        let _inflight_guard = if request.migration_fenced() {
            request
                .route_partition()
                .map(|partition| migration_inflight_enter(&self.migration_inflight, partition))
        } else {
            None
        };
        if self.cluster_router().is_some() && !migration_control {
            if let Some(leader) = cluster_non_serving_redirect(
                &self.cluster_router,
                &self.snapshot_mgr,
                &self.raft_share,
                &self.repl_ctl,
                &request,
            )? {
                debug!(
                    worker_id = self.worker_id,
                    redirect_to = ?leader,
                    "partition rpc redirected to the group leader"
                );
                return Ok(PartitionRpcResponse::Redirect { node_id: leader });
            }
            // Phase 3 in-flight tracking: fenced write variants register
            // BEFORE the fence check, so the cutover's drain observes every
            // decider (register-first closes the check-then-register race).
            // Phase 3 fence: reject writes fenced by an in-flight migration
            // and requests for partitions this worker no longer owns. Runs
            // before the dedup lookup so a retry sees the current state.
            if let Some(fenced) = migration_fence(
                &self.cluster_router,
                &self.meta_mgr,
                self.worker_id,
                &request,
            )
            .await?
            {
                debug!(
                    worker_id = self.worker_id,
                    fenced = ?fenced,
                    "partition rpc rejected by the migration fence"
                );
                return Ok(fenced);
            }
        }
        // Lifecycle commands use durable state; reads need a fresh barrier.
        if matches!(
            request,
            PartitionRpcRequest::QueryTxDecision { .. }
                | PartitionRpcRequest::PrepareTx { .. }
                | PartitionRpcRequest::CommitTx { .. }
                | PartitionRpcRequest::AbortTx { .. }
        ) {
            return self.execute_partition_rpc(request).await;
        }
        if let Some(cached) = self.rpc_dedup.lookup(ctx.src(), ctx.request_id()) {
            debug!(
                worker_id = self.worker_id,
                src = ctx.src(),
                request_id = ctx.request_id(),
                "replayed partition rpc request answered from dedup cache"
            );
            return Ok(cached);
        }
        let response = match self.execute_partition_rpc(request).await {
            Ok(response) => response,
            Err(err) => PartitionRpcResponse::Err(err.to_string()),
        };
        self.rpc_dedup
            .store(ctx.src(), ctx.request_id(), response.clone());
        Ok(response)
    }

    pub(super) async fn execute_partition_rpc(
        &self,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        match request {
            PartitionRpcRequest::QueryTxDecision { coordinator } => {
                Ok(PartitionRpcResponse::TxDecisionStatus(
                    tx_decision_client(&self.tx_decision_client)?
                        .transaction_state(&coordinator)
                        .await?,
                ))
            }
            PartitionRpcRequest::ReadKey {
                table_id,
                partition_id,
                key,
                select,
            } => {
                execute_rpc_read_key(
                    &self.meta_mgr,
                    &self.storage,
                    &self.snapshot_mgr,
                    &self.tx_lock,
                    self.worker_id,
                    table_id,
                    partition_id,
                    key,
                    select,
                )
                .await
            }
            PartitionRpcRequest::ReadRange {
                table_id,
                partition_id,
                start,
                end,
                select,
            } => {
                execute_rpc_read_range(
                    &self.meta_mgr,
                    &self.storage,
                    &self.snapshot_mgr,
                    &self.tx_lock,
                    self.worker_id,
                    table_id,
                    partition_id,
                    start,
                    end,
                    select,
                )
                .await
            }
            write_request @ (PartitionRpcRequest::Insert { .. }
            | PartitionRpcRequest::Delete { .. }
            | PartitionRpcRequest::Update { .. }
            | PartitionRpcRequest::CommitWriteSet { .. }
            | PartitionRpcRequest::LockKeyForUpdate { .. }
            | PartitionRpcRequest::UnlockKeys { .. }) => {
                self.execute_write_rpc(write_request).await
            }
            tx_request @ (PartitionRpcRequest::ApplyCrossPartitionTx { .. }
            | PartitionRpcRequest::PrepareTx { .. }
            | PartitionRpcRequest::CommitTx { .. }
            | PartitionRpcRequest::AbortTx { .. }) => self.execute_tx_rpc(tx_request).await,
            migration_request @ (PartitionRpcRequest::PartitionSnapshotOffer { .. }
            | PartitionRpcRequest::PartitionSnapshotFetch { .. }
            | PartitionRpcRequest::PartitionAdopt { .. }
            | PartitionRpcRequest::PartitionDrop { .. }
            | PartitionRpcRequest::PartitionSplitData { .. }
            | PartitionRpcRequest::PartitionMergeData { .. }
            | PartitionRpcRequest::MigrationFenceQuery { .. }) => {
                self.execute_migration_rpc(migration_request).await
            }
            queue_request @ (PartitionRpcRequest::QueuePrepare { .. }
            | PartitionRpcRequest::QueueReceiveWait { .. }
            | PartitionRpcRequest::QueueWake { .. }) => self.execute_queue_rpc(queue_request).await,
        }
    }
}
