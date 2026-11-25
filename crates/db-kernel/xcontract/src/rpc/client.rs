use super::*;
use ::catalog::contract::queue::QueueWait;

/// Timeout of one partition RPC attempt.
pub(crate) const PARTITION_RPC_TIMEOUT: Duration = Duration::from_secs(10);
/// How many times an idempotent request is attempted under the same
/// idempotency key before giving up.
const MAX_IDEMPOTENT_ATTEMPTS: u32 = 3;
/// Upper bound of redirect re-routings of one logical request.
const MAX_REDIRECTS: u32 = 8;
/// Backoff before re-resolving when the redirect carries no leader (an
/// unresolved election is in progress).
const REDIRECT_UNKNOWN_LEADER_BACKOFF: Duration = Duration::from_millis(50);

/// The classified outcome of one partition-RPC attempt.
enum RetryableOutcome {
    /// Return the response to the caller.
    Respond(PartitionRpcResponse),
    /// Re-resolved the owner; retry against the new target.
    Rerouted(OID),
    /// Retry against the same target (migration window or idempotent
    /// retry).
    Retry,
}

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
    /// recovery records). Cluster mode adds a retry dimension that does not
    /// consume idempotency attempts: redirects and connection-level failures
    /// re-resolve the owner through the directory's serving-node cache.
    pub(super) async fn send_partition_rpc_with_timeout(
        &self,
        target_worker_id: OID,
        request: PartitionRpcRequest,
        timeout: Duration,
    ) -> RS<PartitionRpcResponse> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::PartitionRpc);
        let channel = current_channel()?;
        let request_id = mudu_utils::channel::gen_request_id();
        let mut target_worker_id = target_worker_id;
        let max_attempts = if request.is_idempotent() {
            MAX_IDEMPOTENT_ATTEMPTS
        } else {
            1
        };
        let mut attempt = 0;
        let mut redirects = 0;
        let mut last_error: Option<(ErrorCode, String)> = None;
        loop {
            let endpoint = channel.endpoint::<PartitionRpcRequest>(target_worker_id);
            debug!(
                worker_id = self.worker_id,
                target_worker_id, request_id, attempt, "sending partition rpc request"
            );
            let result = endpoint
                .ask_attempt(request.clone(), request_id, attempt, timeout)
                .await;
            attempt += 1;
            match self
                .handle_retryable_outcome(
                    &request,
                    result,
                    target_worker_id,
                    &mut redirects,
                    &mut last_error,
                )
                .await?
            {
                RetryableOutcome::Respond(response) => {
                    debug!(
                        worker_id = self.worker_id,
                        target_worker_id, request_id, attempt, "received partition rpc response"
                    );
                    return Ok(response);
                }
                RetryableOutcome::Rerouted(new_target) => {
                    target_worker_id = new_target;
                    continue;
                }
                RetryableOutcome::Retry => {
                    debug!(
                        worker_id = self.worker_id,
                        target_worker_id,
                        request_id,
                        attempt,
                        "partition rpc attempt failed; retrying"
                    );
                    if attempt >= max_attempts {
                        let (code, message) = last_error
                            .take()
                            .unwrap_or((ErrorCode::Internal, "no attempt error".to_string()));
                        return Err(mudu_error!(
                            code,
                            format!(
                                "partition rpc attempts exhausted (last error: {message}): server={}, worker={}, target_worker={}, request_id={}, attempts={}",
                                self.server_instance_id,
                                self.worker_id,
                                target_worker_id,
                                request_id,
                                attempt
                            )
                        ));
                    }
                    continue;
                }
            }
        }
    }

    /// Classifies one attempt's outcome: the response to return, or the
    /// retry flow to follow (re-route, wait-out the migration window, or a
    /// plain idempotent retry). Terminal errors surface as `Err`.
    async fn handle_retryable_outcome(
        &self,
        request: &PartitionRpcRequest,
        result: RS<PartitionRpcResponse>,
        target_worker_id: OID,
        redirects: &mut u32,
        last_error: &mut Option<(ErrorCode, String)>,
    ) -> RS<RetryableOutcome> {
        match result {
            Ok(PartitionRpcResponse::Redirect { node_id })
                if self.cluster_router().is_some() && *redirects < MAX_REDIRECTS =>
            {
                *redirects += 1;
                self.note_redirect(request, node_id).await;
                let target = self.reroute_or_stay(request, target_worker_id).await?;
                Ok(RetryableOutcome::Rerouted(target))
            }
            // The partition's placement moved: re-resolve the owner and
            // retry (the request was definitively not executed).
            Ok(PartitionRpcResponse::StaleRoute)
                if self.cluster_router().is_some() && *redirects < MAX_REDIRECTS =>
            {
                *redirects += 1;
                let target = self.reroute_or_stay(request, target_worker_id).await?;
                Ok(RetryableOutcome::Rerouted(target))
            }
            // Cutover write window: the partition's migration fences new
            // writes. Fail fast rather than retry inside the call: an open
            // transaction pins its epoch registration, and the cutover's
            // drain only covers requests on the owner — a retrying caller
            // would hold its registration open for the whole window. The
            // error aborts the transaction; the client retries the whole
            // transaction.
            Ok(PartitionRpcResponse::Migrating) => Err(mudu_error!(
                ErrorCode::ResourceBusy,
                "the partition is migrating; retry the transaction"
            )),
            Ok(response) => Ok(RetryableOutcome::Respond(response)),
            Err(err)
                if self.cluster_router().is_some()
                    && *redirects < MAX_REDIRECTS
                    && matches!(
                        err.ec(),
                        ErrorCode::ObjectNotFound
                            | ErrorCode::Network
                            | ErrorCode::TimedOut
                            | ErrorCode::Tokio
                    ) =>
            {
                // Channel-level failures: ObjectNotFound/Network are
                // definitive non-delivery; TimedOut/Tokio (ask deadline)
                // are uncertain for non-idempotent writes, but
                // re-execution after a failover is safe: an uncommitted
                // write heals away with the dead leader's tail, and a
                // quorum-committed write surfaces as a visible
                // already-exists error either way.
                *redirects += 1;
                self.note_serving_failure(request).await;
                let target = self.reroute_or_stay(request, target_worker_id).await?;
                Ok(RetryableOutcome::Rerouted(target))
            }
            Err(err) => {
                *last_error = Some((err.ec(), err.to_string()));
                Ok(RetryableOutcome::Retry)
            }
        }
    }

    /// Re-resolves the request's owner after a redirect/failure, falling back
    /// to a short backoff with the same target when the leader is unknown.
    async fn reroute_or_stay(&self, request: &PartitionRpcRequest, current_target: OID) -> RS<OID> {
        match self.reroute_target(request).await? {
            Some(new_target) if new_target != current_target => Ok(new_target),
            Some(_) => Ok(current_target),
            None => {
                mudu_sys::sleep(REDIRECT_UNKNOWN_LEADER_BACKOFF).await?;
                Ok(current_target)
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
