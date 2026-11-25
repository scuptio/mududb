//! Thin public-API facade: the `WorkerXContract` methods exposed to the
//! kernel crates, delegating to the free functions in the business modules.
//!
//! Kept separate from [`crate::contract`] so the aggregate struct module does
//! not depend back on the business modules (which would close the module
//! cycle the business modules already open by referencing `WorkerXContract`).

use crate::contract::WorkerXContract;
use crate::queue_activation::{
    QueueActivationInvoker, QueueActivationRegistration, QueueActivationStatus,
};
use crate::queue_types::QueueRuntimeSnapshot;
use ::catalog::contract::worker_snapshot::{KvItem, WorkerSnapshot};
use ::wal::xl_batch::XLBatch;
use ::xengine::tx_mgr::TxMgr;
use mudu::common::id::OID;
use std::sync::Arc;

impl WorkerXContract {
    /// Per-shard runtime stats of this worker for the observability endpoint.
    pub fn queue_runtime_snapshot(&self) -> mudu::common::result::RS<Vec<QueueRuntimeSnapshot>> {
        crate::queue::queue_runtime_snapshot(&self.queue_registry, &self.queue_wait_registry)
    }
}

impl WorkerXContract {
    /// Registers (or replaces) the activation of one queue on this worker.
    pub fn queue_activation_register(
        &self,
        registration: QueueActivationRegistration,
        invoker: Arc<dyn QueueActivationInvoker>,
    ) -> mudu::common::result::RS<()> {
        crate::queue_activation::queue_activation_register(
            &self.activation_registry,
            &self.queue_registry,
            registration,
            invoker,
        )
    }

    /// Removes the activation of `queue_oid` and stops its driver task.
    pub fn queue_activation_unregister(&self, queue_oid: OID) -> mudu::common::result::RS<bool> {
        crate::queue_activation::queue_activation_unregister(&self.activation_registry, queue_oid)
    }

    /// Point-in-time status of every activation registered on this worker.
    pub fn queue_activation_list(&self) -> mudu::common::result::RS<Vec<QueueActivationStatus>> {
        crate::queue_activation::queue_activation_list(&self.activation_registry)
    }
}

impl WorkerXContract {
    /// Begin a new transaction on this worker's snapshot manager.
    pub fn worker_begin_tx(&self) -> mudu::common::result::RS<Arc<dyn TxMgr>> {
        crate::kv::tx::worker_begin_tx(&self.snapshot_mgr)
    }

    /// Begins a statement-shell transaction with no epoch registration.
    pub fn worker_begin_tx_untracked(&self) -> mudu::common::result::RS<Arc<dyn TxMgr>> {
        crate::kv::tx::worker_begin_tx_untracked(&self.snapshot_mgr)
    }

    /// Rolls back the transaction's local state.
    pub fn worker_rollback_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> mudu::common::result::RS<()> {
        crate::kv::tx::worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx_mgr)
    }

    /// The node epoch oracle's current epoch.
    pub fn worker_current_epoch(&self) -> Option<catalog::contract::epoch_oracle::CommitEpoch> {
        crate::kv::tx::worker_current_epoch(&self.snapshot_mgr)
    }

    /// Wait until `epoch` is stable.
    pub async fn worker_wait_epoch_stable(
        &self,
        epoch: catalog::contract::epoch_oracle::CommitEpoch,
    ) -> mudu::common::result::RS<()> {
        crate::kv::tx::worker_wait_epoch_stable(&self.snapshot_mgr, epoch).await
    }
}

impl WorkerXContract {
    /// Read the latest committed value of a KV key.
    pub async fn worker_get_async(&self, key: &[u8]) -> mudu::common::result::RS<Option<Vec<u8>>> {
        crate::kv::kv_ops::worker_get_async(&self.storage, key).await
    }

    /// Read the value of a KV key as visible to `snapshot`.
    pub async fn worker_get_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        key: &[u8],
    ) -> mudu::common::result::RS<Option<Vec<u8>>> {
        crate::kv::kv_ops::worker_get_with_snapshot_async(&self.storage, snapshot, key).await
    }

    /// Scan the latest committed KV items with keys in `[start_key, end_key)`.
    pub async fn worker_range_scan_async(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> mudu::common::result::RS<Vec<KvItem>> {
        crate::kv::kv_ops::worker_range_scan_async(&self.storage, start_key, end_key).await
    }

    /// Scan the KV items visible to `snapshot`.
    pub async fn worker_range_scan_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        start_key: &[u8],
        end_key: &[u8],
    ) -> mudu::common::result::RS<Vec<KvItem>> {
        crate::kv::kv_ops::worker_range_scan_with_snapshot_async(
            &self.storage,
            snapshot,
            start_key,
            end_key,
        )
        .await
    }
}

impl WorkerXContract {
    /// Restores local allocation and applied-read clocks from a verified
    /// business image.
    pub fn observe_snapshot_recovery_watermarks(
        &self,
        water: crate::worker_storage::RecoveryWatermarks,
    ) {
        crate::kv::recovery::observe_snapshot_recovery_watermarks(&self.snapshot_mgr, water)
    }

    /// Replay one WAL batch into storage during recovery.
    pub async fn replay_worker_log_batch(&self, batch: XLBatch) -> mudu::common::result::RS<()> {
        crate::kv::recovery::replay_worker_log_batch(&self.snapshot_mgr, &self.storage, batch).await
    }

    /// Replays a complete serial WAL batch at a verified exclusive end LSN.
    pub async fn replay_worker_log_batch_at(
        &self,
        batch: XLBatch,
        wal_resume: u64,
    ) -> mudu::common::result::RS<()> {
        crate::kv::recovery::replay_worker_log_batch_at(
            &self.snapshot_mgr,
            &self.storage,
            batch,
            wal_resume,
        )
        .await
    }

    /// Mark the end of WAL recovery.
    pub fn finish_worker_log_recovery(&self) -> mudu::common::result::RS<()> {
        crate::kv::recovery::finish_worker_log_recovery(&self.snapshot_mgr)
    }
}

impl WorkerXContract {
    /// Resolves the worker owning `partition_id`.
    pub(crate) async fn resolve_partition_worker(
        &self,
        partition_id: OID,
    ) -> mudu::common::result::RS<Option<OID>> {
        crate::lifecycle_cluster::resolve_partition_worker(
            &self.cluster_router,
            &self.meta_mgr,
            self.default_unpartitioned_worker_id,
            partition_id,
        )
        .await
    }

    /// Installs the partition adoption hook (phase 3).
    pub fn set_partition_adoption(
        &self,
        adoption: Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>,
    ) -> mudu::common::result::RS<()> {
        crate::lifecycle_cluster::set_partition_adoption(&self.partition_adoption, adoption)
    }

    /// The installed partition adoption hook, if any.
    pub(crate) fn partition_adoption(
        &self,
    ) -> Option<Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>> {
        crate::lifecycle_cluster::partition_adoption(&self.partition_adoption)
    }

    /// The cluster router, when the server runs with a cluster topology.
    pub(crate) fn cluster_router(
        &self,
    ) -> Option<&Arc<dyn ::catalog::contract::cluster_router::ClusterRouter>> {
        self.cluster_router.as_ref()
    }
}

impl WorkerXContract {
    /// Installs this worker's data-group coordinator API.
    pub fn set_tx_decision_client(
        &self,
        client: Arc<dyn catalog::contract::tx_decision::TxDecisionClient>,
    ) -> mudu::common::result::RS<()> {
        crate::rpc::coordinator_client::set_tx_decision_client(&self.tx_decision_client, client)
    }

    /// Returns the installed data-group decision API.
    pub fn tx_decision_client(
        &self,
    ) -> mudu::common::result::RS<Arc<dyn catalog::contract::tx_decision::TxDecisionClient>> {
        crate::rpc::coordinator_client::tx_decision_client(&self.tx_decision_client)
    }
}

impl WorkerXContract {
    /// Builds the cross-partition participant list and write set from the
    /// transaction's staged relation ops.
    #[cfg(test)]
    pub(crate) async fn build_cross_partition_tx_ops(
        &self,
        tx: &dyn TxMgr,
    ) -> mudu::common::result::RS<(
        Vec<crate::rpc::CrossPartitionParticipant>,
        Vec<::wal::xl_data_op::XLWrite>,
    )> {
        crate::rpc::cross_partition::build_cross_partition_tx_ops(
            &self.cluster_router,
            &self.meta_mgr,
            self.default_unpartitioned_worker_id,
            self.worker_id,
            tx,
        )
        .await
    }
}
