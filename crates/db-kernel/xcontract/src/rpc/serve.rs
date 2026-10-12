use super::*;
use crate::utils::{
    apply_value_update, project_selected_fields, rpc_bound_as_ref, STATEMENT_LOCK_TIMEOUT,
};

impl WorkerXContract {
    pub(crate) async fn handle_partition_rpc(&self, envelope: Envelope) -> RS<()> {
        debug!(
            worker_id = self.worker_id,
            src = ?envelope.src(),
            msg_id = envelope.msg_id(),
            "received partition rpc request"
        );
        let request = rmp_serde::from_slice::<PartitionRpcRequest>(envelope.payload())
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode partition rpc request error", e))?;
        let response = match self.execute_partition_rpc(request).await {
            Ok(response) => response,
            Err(err) => PartitionRpcResponse::Err(err.to_string()),
        };
        let payload = rmp_serde::to_vec(&response).map_err(|e| {
            mudu_error!(ErrorCode::Encode, "encode partition rpc response error", e)
        })?;
        let bus = current_message_bus()?;
        bus.send(
            *envelope.src(),
            OutgoingMessage::new(PARTITION_RPC_RESPONSE_KIND, payload)
                .with_correlation_id(envelope.msg_id())
                .with_delivery(DeliveryMode::Response),
        )
        .await?;
        debug!(
            worker_id = self.worker_id,
            dst = ?envelope.src(),
            correlation_id = envelope.msg_id(),
            "sent partition rpc response"
        );
        Ok(())
    }

    pub(super) async fn execute_partition_rpc(
        &self,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        match request {
            PartitionRpcRequest::ReadKey {
                table_id,
                partition_id,
                key,
                select,
            } => {
                self.execute_rpc_read_key(table_id, partition_id, key, select)
                    .await
            }
            PartitionRpcRequest::ReadRange {
                table_id,
                partition_id,
                start,
                end,
                select,
            } => {
                self.execute_rpc_read_range(table_id, partition_id, start, end, select)
                    .await
            }
            PartitionRpcRequest::Insert {
                table_id,
                partition_id,
                key,
                value,
            } => {
                self.execute_rpc_insert(table_id, partition_id, key, value)
                    .await
            }
            PartitionRpcRequest::Delete {
                table_id,
                partition_id,
                key,
            } => self.execute_rpc_delete(table_id, partition_id, key).await,
            PartitionRpcRequest::Update {
                table_id,
                partition_id,
                key,
                values,
            } => {
                self.execute_rpc_update(table_id, partition_id, key, values)
                    .await
            }
            PartitionRpcRequest::ApplyCrossPartitionTx {
                tx_id,
                partition_id,
                partition_write_set,
                ..
            } => {
                self.execute_rpc_apply_cross_partition_tx(tx_id, partition_id, partition_write_set)
                    .await
            }
            PartitionRpcRequest::CommitWriteSet {
                tx_id,
                lock_token,
                writes,
            } => {
                self.execute_rpc_commit_write_set(tx_id, lock_token, writes)
                    .await
            }
            PartitionRpcRequest::LockKeyForUpdate {
                lock_token,
                table_id,
                partition_id,
                key,
                select,
            } => {
                self.execute_rpc_lock_key_for_update(
                    lock_token,
                    table_id,
                    partition_id,
                    key,
                    select,
                )
                .await
            }
            PartitionRpcRequest::UnlockKeys { lock_token } => {
                self.execute_rpc_unlock_keys(lock_token).await
            }
        }
    }

    /// Serve `ReadKey`: read the committed value of `key` on `partition_id`
    /// with a fresh snapshot, projected to the `select` columns.
    async fn execute_rpc_read_key(
        &self,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            select_len = select.len(),
            "execute partition rpc read_key"
        );
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        let tx_mgr = self.worker_begin_tx()?;
        let opt_value = self
            .storage
            .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        self.worker_rollback_tx(tx_mgr)?;
        let projected = opt_value
            .map(|value| project_selected_fields(&desc, &key, &value, &VecSelTerm::new(select)))
            .transpose()?;
        Ok(PartitionRpcResponse::ReadKey(projected))
    }

    /// Serve `ReadRange`: scan the committed rows of `partition_id` within
    /// the `start`/`end` bounds with a fresh snapshot, projected to the
    /// `select` columns.
    async fn execute_rpc_read_range(
        &self,
        table_id: OID,
        partition_id: OID,
        start: RpcBound,
        end: RpcBound,
        select: Vec<AttrIndex>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            select_len = select.len(),
            start = ?start,
            end = ?end,
            "execute partition rpc read_range"
        );
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        let tx_mgr = self.worker_begin_tx()?;
        let rows = self
            .storage
            .range_on_partition(
                table_id,
                Some(partition_id),
                (rpc_bound_as_ref(&start), rpc_bound_as_ref(&end)),
                tx_mgr.as_ref(),
            )
            .await?;
        self.worker_rollback_tx(tx_mgr)?;
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

    /// Serve `Insert`: fail when the key exists, otherwise stage and commit
    /// the row through the normal local commit path.
    async fn execute_rpc_insert(
        &self,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            value_len = value.len(),
            "execute partition rpc insert"
        );
        let tx_mgr = self.worker_begin_tx()?;
        let current = self
            .storage
            .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        if current.is_some() {
            self.worker_rollback_tx(tx_mgr)?;
            return Err(mudu_error!(ErrorCode::EntityAlreadyExists, "existing key"));
        }
        self.storage
            .put_on_partition(table_id, Some(partition_id), key, value, tx_mgr.as_ref())
            .await?;
        self.worker_commit_tx_async(tx_mgr).await?;
        Ok(PartitionRpcResponse::Insert)
    }

    /// Serve `Delete`: remove the key through the normal local commit path;
    /// the response carries the number of removed rows.
    async fn execute_rpc_delete(
        &self,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            "execute partition rpc delete"
        );
        let tx_mgr = self.worker_begin_tx()?;
        let deleted = self
            .storage
            .remove_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        self.worker_commit_tx_async(tx_mgr).await?;
        Ok(PartitionRpcResponse::Delete(usize::from(deleted.is_some())))
    }

    /// Serve `Update`: replace the given columns of `key`'s row through the
    /// normal local commit path; the response carries the number of changed
    /// rows.
    async fn execute_rpc_update(
        &self,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            value_pairs = values.len(),
            "execute partition rpc update"
        );
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        let tx_mgr = self.worker_begin_tx()?;
        let current = self
            .storage
            .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        let Some(current) = current else {
            self.worker_rollback_tx(tx_mgr)?;
            return Ok(PartitionRpcResponse::Update(0));
        };
        let updated = apply_value_update(&current, &VecDatum::new(values), &[], &desc)?;
        self.storage
            .put_on_partition(table_id, Some(partition_id), key, updated, tx_mgr.as_ref())
            .await?;
        self.worker_commit_tx_async(tx_mgr).await?;
        Ok(PartitionRpcResponse::Update(1))
    }

    /// Serve `ApplyCrossPartitionTx`: apply one participant partition's
    /// write set of a cross-partition transaction (idempotent per
    /// `(tx_id, partition_id)`).
    async fn execute_rpc_apply_cross_partition_tx(
        &self,
        tx_id: OID,
        partition_id: OID,
        partition_write_set: Vec<XLWrite>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            tx_id,
            partition_id,
            writes = partition_write_set.len(),
            "execute partition rpc apply_cross_partition_tx"
        );
        self.storage
            .apply_cross_partition_tx_async(tx_id, &partition_write_set)
            .await?;
        Ok(PartitionRpcResponse::ApplyCrossPartitionTx)
    }

    /// Serve `CommitWriteSet`: stage the handed-off write set into a fresh
    /// local transaction and commit it, retrying a bounded number of times
    /// on commit-time transaction failures.
    async fn execute_rpc_commit_write_set(
        &self,
        tx_id: OID,
        lock_token: Option<OID>,
        writes: Vec<XLWrite>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            coordinator_tx_id = tx_id,
            writes = writes.len(),
            "execute partition rpc commit_write_set"
        );
        let write_count = writes.len();
        // The write set is fully materialized (final values) and each
        // attempt commits it with a fresh owner-local snapshot, so a
        // commit-time Transaction failure (commit-lock contention or
        // a write-write conflict against a commit that landed inside
        // this RPC's window) is retried a bounded number of times.
        // Retrying does not weaken isolation: the coordinator's
        // snapshot is deliberately not propagated to the owner.
        const MAX_COMMIT_WRITE_SET_ATTEMPTS: u32 = 2;
        let mut attempt = 0;
        let result = loop {
            attempt += 1;
            match self.commit_write_set_once(&writes, lock_token).await {
                Ok(()) => break Ok(()),
                Err(err)
                    if err.ec() == ErrorCode::Transaction
                        && attempt < MAX_COMMIT_WRITE_SET_ATTEMPTS =>
                {
                    debug!(
                        worker_id = self.worker_id,
                        coordinator_tx_id = tx_id,
                        attempt,
                        "commit_write_set retrying after transaction failure"
                    );
                }
                Err(err) => break Err(err),
            }
        };
        // The commit path releases `lock_token`'s locks on every
        // exit; this is the backstop for attempts that failed before
        // reaching it.
        if let Some(token) = lock_token {
            self.tx_lock.release_all(token)?;
        }
        result?;
        Ok(PartitionRpcResponse::CommitWriteSet(write_count))
    }

    /// Serve `LockKeyForUpdate`: take the statement-level write lock on
    /// `key` under `lock_token`, then read the currently committed value
    /// with a fresh snapshot (same as `ReadKey`).
    async fn execute_rpc_lock_key_for_update(
        &self,
        lock_token: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            "execute partition rpc lock_key_for_update"
        );
        let relation_id = PhysicalRelationId {
            table_id,
            partition_id,
        };
        let acquired = self
            .tx_lock
            .lock_some(
                lock_token,
                &[(relation_id, key.clone())],
                STATEMENT_LOCK_TIMEOUT,
            )
            .await?;
        if !acquired {
            return Err(mudu_error!(
                ErrorCode::Transaction,
                "failed to acquire statement locks"
            ));
        }
        // Locked: read the currently committed value with a fresh
        // snapshot (same as ReadKey).
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        let tx_mgr = self.worker_begin_tx()?;
        let opt_value = self
            .storage
            .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        self.worker_rollback_tx(tx_mgr)?;
        let projected = opt_value
            .map(|value| project_selected_fields(&desc, &key, &value, &VecSelTerm::new(select)))
            .transpose()?;
        Ok(PartitionRpcResponse::ReadKey(projected))
    }

    /// Serve `UnlockKeys`: release every statement-level lock held by
    /// `lock_token` on this worker.
    async fn execute_rpc_unlock_keys(&self, lock_token: OID) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            "execute partition rpc unlock_keys"
        );
        self.tx_lock.release_all(lock_token)?;
        Ok(PartitionRpcResponse::UnlockKeys)
    }

    /// Stage a handed-off write set into a fresh local transaction and commit
    /// it through the normal local commit path. When `lock_token` is present
    /// the commit runs under the coordinator's statement locks (re-entrant
    /// acquisition) and they are all released when the commit returns.
    async fn commit_write_set_once(&self, writes: &[XLWrite], lock_token: Option<OID>) -> RS<()> {
        let tx_mgr = self.worker_begin_tx()?;
        let lock_owner = lock_token.unwrap_or(tx_mgr.xid() as OID);
        for write in writes {
            match write {
                XLWrite::Insert(insert) => tx_mgr.put_relation(
                    PhysicalRelationId {
                        table_id: insert.table_id,
                        partition_id: insert.partition_id,
                    },
                    insert.key.clone(),
                    insert.value.clone(),
                ),
                XLWrite::Delete(delete) => tx_mgr.delete_relation(
                    PhysicalRelationId {
                        table_id: delete.table_id,
                        partition_id: delete.partition_id,
                    },
                    delete.key.clone(),
                ),
                XLWrite::Update(_) => {
                    self.worker_rollback_tx(tx_mgr)?;
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "commit_write_set does not support delta updates"
                    ));
                }
            }
        }
        self.worker_commit_tx_with_lock_owner_async(tx_mgr, lock_owner)
            .await
    }
}
