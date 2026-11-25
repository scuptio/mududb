//! Stable group routing for decisions and participant settlement.
use super::*;

impl WorkerXContract {
    /// Learns a redirect: the group of the request's partition is served by
    /// `node_id` (when known).
    pub(super) async fn note_redirect(&self, request: &PartitionRpcRequest, node_id: Option<u64>) {
        if let (Some(router), Some(group), Some(node)) = (
            self.cluster_router(),
            self.request_group(request).await,
            node_id,
        ) {
            router.note_serving_node(group, node);
        }
    }

    pub(super) async fn note_serving_failure(&self, request: &PartitionRpcRequest) {
        if let (Some(router), Some(group)) =
            (self.cluster_router(), self.request_group(request).await)
        {
            if let Some(node) = router.serving_node(group) {
                router.note_serving_failure(group, node);
            }
        }
    }

    async fn request_group(&self, request: &PartitionRpcRequest) -> Option<u64> {
        if let PartitionRpcRequest::PrepareTx { target, .. }
        | PartitionRpcRequest::CommitTx { target, .. }
        | PartitionRpcRequest::AbortTx { target, .. } = request
        {
            return Some(target.group_id);
        }

        if let PartitionRpcRequest::QueryTxDecision { coordinator } = request {
            return Some(coordinator.group_id);
        }
        let partition = request.route_partition()?;
        let (rule, name) = self
            .meta_mgr()
            .find_partition_names(partition)
            .await
            .ok()??;
        self.cluster_router()?
            .placement(&rule, &name)
            .map(|(group, _)| group)
    }

    /// Re-resolves the owner worker of the request's partition through the
    /// (possibly updated) directory.
    pub(super) async fn reroute_target(&self, request: &PartitionRpcRequest) -> RS<Option<OID>> {
        if let PartitionRpcRequest::PrepareTx { target, .. }
        | PartitionRpcRequest::CommitTx { target, .. }
        | PartitionRpcRequest::AbortTx { target, .. } = request
        {
            let router = self.cluster_router().ok_or_else(|| {
                mudu_error!(ErrorCode::InvalidState, "participant directory missing")
            })?;
            let group = target.group_id;
            return router
                .resolve_group_worker(group, target.worker_index)
                .map(Some);
        }

        if let PartitionRpcRequest::QueryTxDecision { coordinator } = request {
            let router = self.cluster_router().ok_or_else(|| {
                mudu_error!(ErrorCode::InvalidState, "coordinator directory missing")
            })?;
            let group = coordinator.group_id;
            return router.resolve_group_worker(group, 0).map(Some);
        }

        let Some(partition_id) = request.route_partition() else {
            return Ok(None);
        };
        self.resolve_partition_worker(partition_id).await
    }
}
