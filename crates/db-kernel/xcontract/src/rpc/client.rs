use super::*;
use ::catalog::contract::queue::QueueWait;

/// Timeout of one partition RPC attempt.
pub(crate) const PARTITION_RPC_TIMEOUT: Duration = Duration::from_secs(10);
/// How many times an idempotent request is attempted under the same
/// idempotency key before giving up.
const MAX_IDEMPOTENT_ATTEMPTS: u32 = 3;

impl WorkerXContract {
    pub(super) async fn send_partition_rpc(
        &self,
        target_worker_id: OID,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        self.send_partition_rpc_with_timeout(target_worker_id, request, PARTITION_RPC_TIMEOUT)
            .await
    }

    /// One logical partition RPC: idempotent requests are retried under the
    /// same idempotency key, non-idempotent ones fail after a single attempt
    /// (their callers resolve the uncertain outcome through the transaction
    /// recovery records).
    pub(super) async fn send_partition_rpc_with_timeout(
        &self,
        target_worker_id: OID,
        request: PartitionRpcRequest,
        timeout: Duration,
    ) -> RS<PartitionRpcResponse> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::PartitionRpc);
        let channel = current_channel()?;
        let endpoint = channel.endpoint::<PartitionRpcRequest>(target_worker_id);
        let request_id = mudu_utils::channel::gen_request_id();
        let max_attempts = if request.is_idempotent() {
            MAX_IDEMPOTENT_ATTEMPTS
        } else {
            1
        };
        let mut attempt = 0;
        loop {
            debug!(
                worker_id = self.worker_id,
                target_worker_id, request_id, attempt, "sending partition rpc request"
            );
            let result = endpoint
                .ask_attempt(request.clone(), request_id, attempt, timeout)
                .await;
            attempt += 1;
            match result {
                Ok(response) => {
                    debug!(
                        worker_id = self.worker_id,
                        target_worker_id, request_id, attempt, "received partition rpc response"
                    );
                    return Ok(response);
                }
                Err(err) if attempt < max_attempts => {
                    debug!(
                        worker_id = self.worker_id,
                        target_worker_id,
                        request_id,
                        attempt,
                        error = %err,
                        "partition rpc attempt failed; retrying"
                    );
                }
                Err(err) => {
                    return Err(mudu_error!(
                        err.ec(),
                        format!(
                            "partition rpc failed: server={}, worker={}, target_worker={}, request_id={}, attempts={}",
                            self.server_instance_id,
                            self.worker_id,
                            target_worker_id,
                            request_id,
                            attempt
                        ),
                        err
                    ));
                }
            }
        }
    }

    pub(crate) async fn remote_read_key(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::ReadKey {
                    table_id,
                    partition_id,
                    key,
                    select,
                },
            )
            .await?
        {
            PartitionRpcResponse::ReadKey(value) => Ok(value),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected read_key rpc response"
            )),
        }
    }

    pub(crate) async fn remote_read_range(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        start: RpcBound,
        end: RpcBound,
        select: Vec<AttrIndex>,
    ) -> RS<Vec<Vec<Option<DataBin>>>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::ReadRange {
                    table_id,
                    partition_id,
                    start,
                    end,
                    select,
                },
            )
            .await?
        {
            PartitionRpcResponse::ReadRange(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected read_range rpc response"
            )),
        }
    }

    /// Take a statement-level write lock on `key` at the owning worker under
    /// `lock_token` and return the currently committed value (projected),
    /// like `remote_read_key`.
    pub(crate) async fn remote_lock_key_for_update(
        &self,
        target_worker_id: OID,
        lock_token: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::LockKeyForUpdate {
                    lock_token,
                    table_id,
                    partition_id,
                    key,
                    select,
                },
            )
            .await?
        {
            PartitionRpcResponse::ReadKey(value) => Ok(value),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected lock_key_for_update rpc response"
            )),
        }
    }

    /// Release all statement-level locks held by `lock_token` on the given
    /// owner worker (rollback path).
    pub(crate) async fn remote_unlock_keys(
        &self,
        target_worker_id: OID,
        lock_token: OID,
    ) -> RS<()> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::UnlockKeys { lock_token },
            )
            .await?
        {
            PartitionRpcResponse::UnlockKeys => Ok(()),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected unlock_keys rpc response"
            )),
        }
    }

    /// Send a `CommitWriteSet` handoff to the worker owning every staged
    /// partition of a transaction. Returns the number of writes applied by
    /// the owner.
    pub(crate) async fn remote_commit_write_set(
        &self,
        target_worker_id: OID,
        tx_id: OID,
        lock_token: Option<OID>,
        writes: Vec<XLWrite>,
    ) -> RS<usize> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::CommitWriteSet {
                    tx_id,
                    lock_token,
                    writes,
                },
            )
            .await?
        {
            PartitionRpcResponse::CommitWriteSet(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected commit_write_set rpc response"
            )),
        }
    }

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

    /// Ask the shard owner to load the shard's runtime state (rebuild the
    /// counters, migrate the legacy watermark, re-deliver crashed claims).
    pub(crate) async fn remote_queue_prepare(&self, shard: &QueueShard) -> RS<()> {
        match self
            .send_partition_rpc(
                shard.worker_id,
                PartitionRpcRequest::QueuePrepare {
                    queue_oid: shard.queue_oid,
                    partition_id: shard.partition_id,
                },
            )
            .await?
        {
            PartitionRpcResponse::QueuePrepare => Ok(()),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected queue_prepare rpc response"
            )),
        }
    }

    /// Dispatch a `RECEIVE ... WAIT` loop to the shard owner.
    pub(crate) async fn remote_queue_receive_wait(
        &self,
        shard: &QueueShard,
        count: u64,
        wait: QueueWait,
        consumer: Option<String>,
    ) -> RS<Vec<Vec<Option<Buf>>>> {
        let (wait_ms, timeout) = match wait {
            QueueWait::Timeout(ms) => (
                Some(ms),
                Duration::from_millis(ms).saturating_add(PARTITION_RPC_TIMEOUT),
            ),
            // No wall-clock bound: the owner parks on its wait registry and
            // the caller's own cancellation drops the ask.
            QueueWait::Forever => (None, Duration::from_secs(24 * 3600)),
        };
        match self
            .send_partition_rpc_with_timeout(
                shard.worker_id,
                PartitionRpcRequest::QueueReceiveWait {
                    queue_oid: shard.queue_oid,
                    partition_id: shard.partition_id,
                    count,
                    wait_ms,
                    consumer,
                },
                timeout,
            )
            .await?
        {
            PartitionRpcResponse::QueueReceiveWait(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected queue_receive_wait rpc response"
            )),
        }
    }

    /// Post-commit notification to the shard owner: publish the committed
    /// watermark and wake parked receivers/activations.
    pub(crate) async fn send_queue_wake(
        &self,
        target_worker_id: OID,
        queue_oid: OID,
        partition_id: OID,
        state: Option<(u64, u64, u64, u64)>,
        consumers_changed: bool,
    ) -> RS<()> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::QueueWake {
                    queue_oid,
                    partition_id,
                    state,
                    consumers_changed,
                },
            )
            .await?
        {
            PartitionRpcResponse::QueueWake => Ok(()),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected queue_wake rpc response"
            )),
        }
    }
}
