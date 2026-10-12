use super::*;

impl WorkerXContract {
    pub(super) async fn send_partition_rpc(
        &self,
        target_worker_id: OID,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::PartitionRpc);
        debug!(
            worker_id = self.worker_id,
            target_worker_id,
            request = ?request,
            "sending partition rpc request"
        );
        let bus = current_message_bus()?;
        let payload = rmp_serde::to_vec(&request)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode partition rpc request error", e))?;
        let msg_id = bus
            .send(
                target_worker_id,
                OutgoingMessage::new(PARTITION_RPC_REQUEST_KIND, payload)
                    .with_delivery(DeliveryMode::Request),
            )
            .await?;
        debug!(
            worker_id = self.worker_id,
            target_worker_id, msg_id, "waiting partition rpc response"
        );
        let envelope = mudu_sys::task::async_::timeout(
            Duration::from_secs(10),
            bus.recv(RecvFilter {
                src: Some(target_worker_id),
                dst: Some(self.worker_id),
                kind: Some(PARTITION_RPC_RESPONSE_KIND),
                correlation_id: Some(msg_id),
            }),
        )
        .await
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::Tokio,
                format!(
                    "partition rpc response timeout: server={}, worker={}, target_worker={}, msg_id={}",
                    self.server_instance_id, self.worker_id, target_worker_id, msg_id
                )
            )
        })??;
        debug!(
            worker_id = self.worker_id,
            target_worker_id,
            msg_id,
            received_msg_id = envelope.msg_id(),
            received_correlation_id = ?envelope.correlation_id(),
            "received partition rpc response envelope"
        );
        rmp_serde::from_slice(envelope.payload())
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode partition rpc response error", e))
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
}
