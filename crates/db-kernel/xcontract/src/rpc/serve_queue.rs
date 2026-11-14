//! Partition-RPC handlers of the queue-family requests (`QueuePrepare`,
//! `QueueReceiveWait`, `QueueWake`).
use super::*;
use ::catalog::contract::queue::QueueWait;

impl WorkerXContract {
    /// Dispatches one queue-family partition-RPC request.
    pub(super) async fn execute_queue_rpc(
        &self,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        match request {
            PartitionRpcRequest::QueuePrepare {
                queue_oid,
                partition_id,
            } => {
                self.execute_rpc_queue_prepare(queue_oid, partition_id)
                    .await
            }
            PartitionRpcRequest::QueueReceiveWait {
                queue_oid,
                partition_id,
                count,
                wait_ms,
                consumer,
            } => {
                self.execute_rpc_queue_receive_wait(
                    queue_oid,
                    partition_id,
                    count,
                    wait_ms,
                    consumer,
                )
                .await
            }
            PartitionRpcRequest::QueueWake {
                queue_oid,
                partition_id,
                state,
                consumers_changed,
            } => {
                self.execute_rpc_queue_wake(queue_oid, partition_id, state, consumers_changed)
                    .await
            }
            other => Err(mudu_error!(
                ErrorCode::Internal,
                format!("not a queue partition rpc: {other:?}")
            )),
        }
    }

    /// Serve `QueuePrepare`: load the shard's runtime state (rebuild the
    /// counters, migrate the legacy watermark, re-deliver crashed claims) on
    /// this worker, which must own the shard.
    async fn execute_rpc_queue_prepare(
        &self,
        queue_oid: OID,
        partition_id: OID,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            queue_oid, partition_id, "execute partition rpc queue_prepare"
        );
        let shard = self
            .resolve_local_queue_shard(queue_oid, partition_id)
            .await?;
        self.ensure_shard_runtime_loaded(&shard).await?;
        self.ensure_consumer_state_loaded(queue_oid, shard.shard_index)
            .await?;
        Ok(PartitionRpcResponse::QueuePrepare)
    }

    /// Serve `QueueReceiveWait`: run the owner-side WAIT loop (autonomous
    /// claim rounds committing between parks on the local wait registry).
    async fn execute_rpc_queue_receive_wait(
        &self,
        queue_oid: OID,
        partition_id: OID,
        count: u64,
        wait_ms: Option<u64>,
        consumer: Option<String>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            queue_oid, partition_id, count, "execute partition rpc queue_receive_wait"
        );
        let shard = self
            .resolve_local_queue_shard(queue_oid, partition_id)
            .await?;
        self.ensure_shard_runtime_loaded(&shard).await?;
        if consumer.is_some() {
            self.ensure_consumer_state_loaded(queue_oid, shard.shard_index)
                .await?;
        }
        let wait = match wait_ms {
            Some(ms) => QueueWait::Timeout(ms),
            None => QueueWait::Forever,
        };
        let rows = self
            .receive_wait_loop(&shard, count, wait, consumer)
            .await?;
        Ok(PartitionRpcResponse::QueueReceiveWait(rows))
    }

    /// Serve `QueueWake`: publish the committed watermark, refresh the
    /// consumer cursor cache when the consumer set changed, and wake parked
    /// receivers/activations of the shard.
    async fn execute_rpc_queue_wake(
        &self,
        queue_oid: OID,
        partition_id: OID,
        state: Option<(u64, u64, u64, u64)>,
        consumers_changed: bool,
    ) -> RS<PartitionRpcResponse> {
        let shard = self
            .resolve_local_queue_shard(queue_oid, partition_id)
            .await?;
        if let Some((next, frontier, depth, bytes)) = state {
            self.publish_queue_state(shard_key(&shard), next, frontier, depth, bytes)?;
        }
        if consumers_changed {
            self.reload_consumer_state(queue_oid, shard.shard_index)
                .await?;
            self.queue_wait_registry.notify_all(shard_key(&shard))?;
        }
        self.queue_wait_registry.notify_one(shard_key(&shard))?;
        if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
            self.activation_registry.notify(queue_oid)?;
        }
        Ok(PartitionRpcResponse::QueueWake)
    }
}
