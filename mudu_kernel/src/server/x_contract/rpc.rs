use super::utils::*;
use super::*;
use crate::wal::log_frame::frame_lsns;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct CrossPartitionParticipant {
    partition_id: OID,
    worker_id: OID,
}

fn cross_partition_wal_ops(write_set: &[XLWrite]) -> Vec<TxOp> {
    let mut ops = Vec::with_capacity(write_set.len() + 2);
    ops.push(TxOp::Begin);
    ops.extend(write_set.iter().cloned().map(TxOp::Write));
    ops.push(TxOp::Commit);
    ops
}

fn partition_write_set(write_set: &[XLWrite], partition_id: OID) -> Vec<XLWrite> {
    write_set
        .iter()
        .filter(|write| write.partition_id() == partition_id)
        .cloned()
        .collect()
}

/// Flatten staged relation writes into a WAL-style write set: staged values
/// become `XLWrite::Insert`, staged deletes become `XLWrite::Delete`.
fn staged_write_set(
    staged: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
) -> Vec<XLWrite> {
    let mut write_set = Vec::new();
    for (relation_id, rows) in staged {
        for (key, value) in rows {
            match value {
                Some(value) => write_set.push(XLWrite::Insert(XLInsert {
                    table_id: relation_id.table_id,
                    partition_id: relation_id.partition_id,
                    tuple_id: 0,
                    key: key.clone(),
                    value: value.clone(),
                })),
                None => write_set.push(XLWrite::Delete(XLDelete {
                    table_id: relation_id.table_id,
                    partition_id: relation_id.partition_id,
                    tuple_id: 0,
                    key: key.clone(),
                })),
            }
        }
    }
    write_set
}

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

    async fn execute_partition_rpc(
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
                    .map(|value| {
                        project_selected_fields(&desc, &key, &value, &VecSelTerm::new(select))
                    })
                    .transpose()?;
                Ok(PartitionRpcResponse::ReadKey(projected))
            }
            PartitionRpcRequest::ReadRange {
                table_id,
                partition_id,
                start,
                end,
                select,
            } => {
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
            PartitionRpcRequest::Insert {
                table_id,
                partition_id,
                key,
                value,
            } => {
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
            PartitionRpcRequest::Delete {
                table_id,
                partition_id,
                key,
            } => {
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
            PartitionRpcRequest::Update {
                table_id,
                partition_id,
                key,
                values,
            } => {
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
                let updated = apply_value_update(&current, &VecDatum::new(values), &desc)?;
                self.storage
                    .put_on_partition(table_id, Some(partition_id), key, updated, tx_mgr.as_ref())
                    .await?;
                self.worker_commit_tx_async(tx_mgr).await?;
                Ok(PartitionRpcResponse::Update(1))
            }
            PartitionRpcRequest::ApplyCrossPartitionTx {
                tx_id,
                coordinator_worker_id: _,
                partition_id,
                visibility_epoch: _,
                partition_write_set,
            } => {
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
            PartitionRpcRequest::CommitWriteSet {
                tx_id,
                lock_token,
                writes,
            } => {
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
            PartitionRpcRequest::LockKeyForUpdate {
                lock_token,
                table_id,
                partition_id,
                key,
                select,
            } => {
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
                    .map(|value| {
                        project_selected_fields(&desc, &key, &value, &VecSelTerm::new(select))
                    })
                    .transpose()?;
                Ok(PartitionRpcResponse::ReadKey(projected))
            }
            PartitionRpcRequest::UnlockKeys { lock_token } => {
                debug!(
                    worker_id = self.worker_id,
                    "execute partition rpc unlock_keys"
                );
                self.tx_lock.release_all(lock_token)?;
                Ok(PartitionRpcResponse::UnlockKeys)
            }
        }
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

    async fn send_partition_rpc(
        &self,
        target_worker_id: OID,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        let _stage = crate::server::stage_stats::StageGuard::new(
            crate::server::stage_stats::Stage::PartitionRpc,
        );
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

    async fn remote_apply_cross_partition_tx(
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

    /// Commit `tx`, routing its staged writes by owning worker:
    ///
    /// - empty transactions and transactions with worker-local KV writes use
    ///   the normal local commit (`worker_commit_tx_async`);
    /// - when every staged relation write is owned by the same remote worker,
    ///   the write set is handed off to that owner via `CommitWriteSet` and
    ///   committed there as a local transaction (single WAL append/flush on
    ///   the owner), then the local transaction is cleaned up;
    /// - anything else (mixed local/remote writes or multiple remote owners)
    ///   uses the cross-partition commit path.
    ///
    /// A transaction that mixes KV writes with relation writes owned by other
    /// workers is rejected: committing it locally would apply those relation
    /// writes to the wrong worker's storage.
    pub(crate) async fn worker_commit_routed_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        if tx.is_empty() {
            return self.worker_commit_tx_async(tx).await;
        }
        if !tx.staged_put_items().is_empty() {
            let staged = tx.staged_relation_ops();
            for relation_id in staged.keys() {
                let owner = self
                    .resolve_partition_worker(relation_id.partition_id)
                    .await?
                    .unwrap_or(self.worker_id);
                if owner != self.worker_id {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "transactions mixing KV writes with relation writes owned by other workers are not supported"
                    ));
                }
            }
            return self.worker_commit_tx_async(tx).await;
        }
        let staged = tx.staged_relation_ops();
        let mut partition_owners = BTreeMap::new();
        for relation_id in staged.keys() {
            let owner = self
                .resolve_partition_worker(relation_id.partition_id)
                .await?
                .unwrap_or(self.worker_id);
            partition_owners.insert(relation_id.partition_id, owner);
        }
        if !is_cross_partition_tx(tx.as_ref(), self.worker_id, &partition_owners) {
            return self.worker_commit_tx_async(tx).await;
        }
        let mut has_local_writes = false;
        let mut remote_owners = BTreeSet::new();
        for owner in partition_owners.values() {
            if *owner == self.worker_id {
                has_local_writes = true;
            } else {
                remote_owners.insert(*owner);
            }
        }
        if !has_local_writes && remote_owners.len() == 1 {
            if let Some(owner) = remote_owners.iter().next().copied() {
                return self.handoff_commit_tx_async(tx, owner, &staged).await;
            }
        }
        // Mixed local/remote or multiple owners: the cross-partition apply
        // path does not take the owner's XLockMgr, so statement locks held on
        // remote owners would neither protect this commit nor be released by
        // it. Release them up front (this route keeps the pre-pessimistic
        // semantics); local statement locks stay and are released by the
        // commit/rollback cleanup.
        let owners = tx.remote_lock_owners();
        if !owners.is_empty() {
            let token = statement_lock_token(self.worker_id, tx.xid());
            for owner in owners {
                if let Err(err) = self.remote_unlock_keys(owner, token).await {
                    debug!(
                        worker_id = self.worker_id,
                        owner, "remote unlock before cross-partition commit failed: {err}"
                    );
                }
            }
            tx.clear_remote_lock_owners();
        }
        self.worker_commit_cross_partition_tx_async(tx).await
    }

    /// Hand the staged write set of `tx` over to `owner_worker_id`, which
    /// commits it as its own local transaction under the coordinator's
    /// statement locks. On success the owner has released those locks; on
    /// failure they are released remotely and the local transaction is
    /// aborted. No local WAL or commit locks are involved on this worker.
    async fn handoff_commit_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
        owner_worker_id: OID,
        staged: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    ) -> RS<()> {
        let writes = staged_write_set(staged);
        let lock_token = statement_lock_token(self.worker_id, tx.xid());
        debug!(
            worker_id = self.worker_id,
            owner_worker_id,
            tx_id = tx.xid(),
            writes = writes.len(),
            "handing off write set to owner worker"
        );
        let result = self
            .remote_commit_write_set(owner_worker_id, tx.xid() as OID, Some(lock_token), writes)
            .await;
        match result {
            Ok(_) => {
                tx.clear_remote_lock_owners();
                self.worker_rollback_tx(tx)
            }
            Err(err) => {
                // The owner did not commit: drop the statement locks it
                // still holds for this transaction, then abort locally.
                let abort = self.worker_abort_tx_async(tx).await;
                abort?;
                Err(err)
            }
        }
    }

    pub(crate) async fn worker_commit_cross_partition_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
    ) -> RS<()> {
        let xid = tx.xid();
        tx.build_write_ops();
        let write_ops = tx.write_ops();
        acquire_commit_locks(&self.tx_lock, xid as OID, &write_ops).await?;

        let result = async {
            let _prepared = self.storage.prepare_commit_async(tx.as_ref()).await?;
            let (participants, write_set) = self.build_cross_partition_tx_ops(tx.as_ref()).await?;
            // Enqueue (allocating LSNs) inside the commit-lock critical
            // section so WAL order matches apply order, but defer the flush
            // drive and the durability wait until after the locks are
            // released.
            let mut last_lsn = None;
            if let Some(log) = self.log_cloned()? {
                let batch = XLBatch::new(vec![XLEntry {
                    xid,
                    ops: cross_partition_wal_ops(&write_set),
                }]);
                let frames = log.serialize_entry(&batch)?;
                let lsns = frame_lsns(&frames)?;
                // Non-force enqueue: share fsyncs through the group-commit
                // window instead of forcing one per commit.
                last_lsn = Some(log.enqueue_group_commit(frames, lsns, false).await?);
            }
            self.apply_cross_partition_ops(xid as OID, participants, write_set)
                .await
                .map(|()| last_lsn)
        }
        .await;

        self.tx_lock.release(xid as OID, &write_ops)?;
        // Drive the group-commit flush round outside the commit-lock
        // critical section, same as the local commit path (see
        // x_contract/kv.rs): the inline write+fsync must not be serialized
        // behind the commit locks.
        if let Some(log) = self.log_cloned()? {
            log.drive_group_commit_flush().await?;
        }
        self.worker_rollback_tx(tx)?;
        let last_lsn = result?;
        if let (Some(log), Some(last_lsn)) = (self.log_cloned()?, last_lsn) {
            log.wait_group_commit_advanced(last_lsn).await?;
        }
        Ok(())
    }

    async fn build_cross_partition_tx_ops(
        &self,
        tx: &dyn TxMgr,
    ) -> RS<(Vec<CrossPartitionParticipant>, Vec<XLWrite>)> {
        let staged = tx.staged_relation_ops();
        let mut participants = BTreeMap::new();
        for relation_id in staged.keys() {
            let worker_id = self
                .resolve_partition_worker(relation_id.partition_id)
                .await?
                .unwrap_or(self.worker_id);
            participants.insert(relation_id.partition_id, worker_id);
        }
        Ok((
            participants
                .into_iter()
                .map(|(partition_id, worker_id)| CrossPartitionParticipant {
                    partition_id,
                    worker_id,
                })
                .collect(),
            staged_write_set(&staged),
        ))
    }

    async fn apply_cross_partition_ops(
        &self,
        tx_id: OID,
        participants: Vec<CrossPartitionParticipant>,
        write_set: Vec<XLWrite>,
    ) -> RS<()> {
        for participant in &participants {
            let writes = partition_write_set(&write_set, participant.partition_id);
            if participant.worker_id != self.worker_id {
                self.remote_apply_cross_partition_tx(
                    participant.worker_id,
                    tx_id,
                    participant.partition_id,
                    tx_id as u64,
                    writes,
                )
                .await?;
            } else {
                self.storage
                    .apply_cross_partition_tx_async(tx_id, &writes)
                    .await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]
mod tests {
    use super::*;
    use crate::contract::schema_column::SchemaColumn;
    use crate::server::message_bus_api::{
        set_current_message_bus, unset_current_message_bus, DeliveryMode, Envelope, MessageBus,
        MessageId, OutgoingMessage, RecvFilter, SubscriptionId,
    };
    use crate::server::test_meta_mgr::TestMetaMgr;
    use crate::x_engine::tx_mgr::PhysicalRelationId;
    use async_trait::async_trait;
    use mudu_sys::env_var::temp_dir;
    use mudu_sys::sync::SMutex;
    use mudu_type::data_type_fn_param::DataType;
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::type_family::TypeFamily;
    use mudu_utils::oid::gen_oid;
    use std::collections::VecDeque;
    use std::sync::Arc;

    fn test_schema() -> crate::contract::schema_table::SchemaTable {
        // Use a fixed OID so helper functions that rebuild the schema return the same table id.
        crate::contract::schema_table::SchemaTable::new_with_oid(
            42,
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
            ],
            vec![0],
            vec![1],
        )
    }

    fn meta_table(schema: &crate::contract::schema_table::SchemaTable) -> RS<Arc<TableDesc>> {
        crate::contract::table_info::TableInfo::new(schema.clone())?.table_desc()
    }

    fn datum(v: i32) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    fn key_row(v: i32) -> VecDatum {
        VecDatum::new(vec![(0, datum(v))])
    }

    fn value_row(v: i32) -> VecDatum {
        VecDatum::new(vec![(1, datum(v))])
    }

    async fn make_contract() -> WorkerXContract {
        make_contract_with_worker(0, 0).await
    }

    async fn make_contract_with_worker(
        worker_id: OID,
        default_unpartitioned_worker_id: OID,
    ) -> WorkerXContract {
        let data_dir = temp_dir()
            .join(format!("rpc_test_{}", gen_oid()))
            .to_string_lossy()
            .to_string();
        let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr: Arc::new(TestMetaMgr::new()),
            log: None,
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id,
            default_unpartitioned_worker_id,
            partition_id: 0,
            data_dir,
            async_runtime: None,
            server_instance_id: 0,
        })
        .unwrap();
        let schema = test_schema();
        let tx = contract.begin_tx().await.unwrap();
        contract.create_table(tx.clone(), &schema).await.unwrap();
        contract.commit_tx(tx).await.unwrap();
        contract
    }

    fn table_id() -> OID {
        test_schema().id()
    }

    #[test]
    fn cross_partition_wal_ops_empty_is_begin_commit() {
        let ops = cross_partition_wal_ops(&[]);
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], TxOp::Begin));
        assert!(matches!(ops[1], TxOp::Commit));
    }

    #[test]
    fn cross_partition_wal_ops_with_writes_preserves_order() {
        let writes = vec![
            XLWrite::Insert(XLInsert {
                table_id: 1,
                partition_id: 0,
                tuple_id: 0,
                key: b"a".to_vec(),
                value: b"1".to_vec(),
            }),
            XLWrite::Delete(XLDelete {
                table_id: 2,
                partition_id: 0,
                tuple_id: 0,
                key: b"b".to_vec(),
            }),
        ];
        let ops = cross_partition_wal_ops(&writes);
        assert_eq!(ops.len(), 4);
        assert!(matches!(ops[0], TxOp::Begin));
        assert!(matches!(ops[1], TxOp::Write(_)));
        assert!(matches!(ops[2], TxOp::Write(_)));
        assert!(matches!(ops[3], TxOp::Commit));
    }

    #[test]
    fn partition_write_set_filters_by_partition_id_preserving_order() {
        let writes = vec![
            XLWrite::Insert(XLInsert {
                table_id: 1,
                partition_id: 1,
                tuple_id: 0,
                key: b"a".to_vec(),
                value: b"1".to_vec(),
            }),
            XLWrite::Insert(XLInsert {
                table_id: 2,
                partition_id: 2,
                tuple_id: 0,
                key: b"b".to_vec(),
                value: b"2".to_vec(),
            }),
            XLWrite::Delete(XLDelete {
                table_id: 3,
                partition_id: 1,
                tuple_id: 0,
                key: b"c".to_vec(),
            }),
        ];
        let filtered = partition_write_set(&writes, 1);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].partition_id(), 1);
        assert_eq!(filtered[1].partition_id(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_partition_rpc_read_key_missing() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let request = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(request).await.unwrap();
        assert_eq!(response, PartitionRpcResponse::ReadKey(None));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_partition_rpc_insert_then_read_key() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let insert = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
            value: value.clone(),
        };
        assert_eq!(
            contract.execute_partition_rpc(insert).await.unwrap(),
            PartitionRpcResponse::Insert
        );

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_partition_rpc_insert_duplicate_returns_entity_already_exists() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let insert = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
            value: value.clone(),
        };
        contract
            .execute_partition_rpc(insert.clone())
            .await
            .unwrap();
        let result = contract.execute_partition_rpc(insert).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().ec(), ErrorCode::EntityAlreadyExists);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_partition_rpc_delete_existing_and_missing() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let insert = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
            value,
        };
        contract.execute_partition_rpc(insert).await.unwrap();

        let delete = PartitionRpcRequest::Delete {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
        };
        assert_eq!(
            contract
                .execute_partition_rpc(delete.clone())
                .await
                .unwrap(),
            PartitionRpcResponse::Delete(1)
        );
        assert_eq!(
            contract.execute_partition_rpc(delete).await.unwrap(),
            PartitionRpcResponse::Delete(0)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_partition_rpc_update_existing_and_missing() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let update = PartitionRpcRequest::Update {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
            values: vec![(1, datum(20))],
        };
        assert_eq!(
            contract
                .execute_partition_rpc(update.clone())
                .await
                .unwrap(),
            PartitionRpcResponse::Update(0)
        );

        let insert = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
            value,
        };
        contract.execute_partition_rpc(insert).await.unwrap();

        assert_eq!(
            contract.execute_partition_rpc(update).await.unwrap(),
            PartitionRpcResponse::Update(1)
        );

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(20))]))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_partition_rpc_read_range() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key1 = build_key_tuple(&key_row(1), &desc).unwrap();
        let key2 = build_key_tuple(&key_row(2), &desc).unwrap();
        let value1 = build_value_tuple(&value_row(10), &desc).unwrap();
        let value2 = build_value_tuple(&value_row(20), &desc).unwrap();

        for (key, value) in [(key1.clone(), value1), (key2.clone(), value2)] {
            let insert = PartitionRpcRequest::Insert {
                table_id: table_id(),
                partition_id: 0,
                key,
                value,
            };
            contract.execute_partition_rpc(insert).await.unwrap();
        }

        let request = PartitionRpcRequest::ReadRange {
            table_id: table_id(),
            partition_id: 0,
            start: RpcBound::Included(key1),
            end: RpcBound::Unbounded,
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(request).await.unwrap();
        let PartitionRpcResponse::ReadRange(rows) = response else {
            panic!("expected ReadRange response");
        };
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_partition_rpc_apply_cross_partition_tx() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let request = PartitionRpcRequest::ApplyCrossPartitionTx {
            tx_id: 42,
            coordinator_worker_id: 0,
            partition_id: 0,
            visibility_epoch: 0,
            partition_write_set: vec![XLWrite::Insert(XLInsert {
                table_id: table_id(),
                partition_id: 0,
                tuple_id: 0,
                key,
                value,
            })],
        };
        assert_eq!(
            contract.execute_partition_rpc(request).await.unwrap(),
            PartitionRpcResponse::ApplyCrossPartitionTx
        );
    }

    struct MockMessageBus {
        local_endpoint: OID,
        sent: SMutex<Vec<(OID, OutgoingMessage)>>,
        responses: SMutex<VecDeque<Envelope>>,
    }

    impl MockMessageBus {
        fn new(local_endpoint: OID) -> Self {
            Self {
                local_endpoint,
                sent: SMutex::new(Vec::new()),
                responses: SMutex::new(VecDeque::new()),
            }
        }

        fn push_response(&self, envelope: Envelope) {
            self.responses.lock().unwrap().push_back(envelope);
        }

        fn take_sent(&self) -> Vec<(OID, OutgoingMessage)> {
            self.sent.lock().unwrap().drain(..).collect()
        }
    }

    #[async_trait]
    impl MessageBus for MockMessageBus {
        fn local_endpoint(&self) -> OID {
            self.local_endpoint
        }

        async fn send(&self, dst: OID, message: OutgoingMessage) -> RS<MessageId> {
            self.sent.lock().unwrap().push((dst, message));
            Ok(1)
        }

        async fn recv(&self, filter: RecvFilter) -> RS<Envelope> {
            let response = {
                let mut responses = self.responses.lock().unwrap();
                let mut found = None;
                for (index, envelope) in responses.iter().enumerate() {
                    if envelope.matches(&filter) {
                        found = Some(responses.remove(index).unwrap());
                        break;
                    }
                }
                found
            };
            match response {
                Some(envelope) => Ok(envelope),
                // Block forever when no matching response is queued.
                None => std::future::pending().await,
            }
        }

        fn on_recv_callback(
            &self,
            _filter: RecvFilter,
            _callback: crate::server::message_bus_api::OnRecvCallback,
        ) -> RS<SubscriptionId> {
            unimplemented!()
        }

        fn cancel_callback(&self, _id: SubscriptionId) -> RS<bool> {
            unimplemented!()
        }
    }

    fn response_envelope(
        msg_id: MessageId,
        src: OID,
        dst: OID,
        response: PartitionRpcResponse,
    ) -> Envelope {
        Envelope::new(
            msg_id + 100,
            Some(msg_id),
            src,
            dst,
            PARTITION_RPC_RESPONSE_KIND,
            rmp_serde::to_vec(&response).unwrap(),
            DeliveryMode::Response,
        )
    }

    fn read_key_request() -> PartitionRpcRequest {
        PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: b"k".to_vec(),
            select: vec![],
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn send_partition_rpc_sends_request_and_decodes_response() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        let response = PartitionRpcResponse::ReadKey(Some(vec![Some(b"v".to_vec())]));
        bus.push_response(response_envelope(1, 2, 0, response.clone()));

        let result = contract
            .send_partition_rpc(2, read_key_request())
            .await
            .unwrap();
        assert_eq!(result, response);

        let sent = bus.take_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 2);
        assert_eq!(sent[0].1.kind(), PARTITION_RPC_REQUEST_KIND);
        let decoded = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
        assert!(matches!(decoded, PartitionRpcRequest::ReadKey { .. }));

        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn send_partition_rpc_times_out() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;

        let result = contract.send_partition_rpc(2, read_key_request()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_read_key_matches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        let response = PartitionRpcResponse::ReadKey(Some(vec![Some(b"v".to_vec())]));
        bus.push_response(response_envelope(1, 2, 0, response));

        let result = contract
            .remote_read_key(2, table_id(), 0, b"k".to_vec(), vec![])
            .await
            .unwrap();
        assert_eq!(result, Some(vec![Some(b"v".to_vec())]));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_read_key_mismatches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Insert));

        let result = contract
            .remote_read_key(2, table_id(), 0, b"k".to_vec(), vec![])
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unexpected read_key rpc response"));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn handle_partition_rpc_sends_response() {
        let contract = make_contract().await;
        let bus = Arc::new(MockMessageBus::new(contract.worker_id()));
        set_current_message_bus(bus.clone());

        let request = read_key_request();
        let payload = rmp_serde::to_vec(&request).unwrap();
        let envelope = Envelope::new(
            1,
            None,
            2,
            contract.worker_id(),
            PARTITION_RPC_REQUEST_KIND,
            payload,
            DeliveryMode::Request,
        );
        contract.handle_partition_rpc(envelope).await.unwrap();

        let sent = bus.take_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 2);
        assert_eq!(sent[0].1.kind(), PARTITION_RPC_RESPONSE_KIND);
        assert_eq!(sent[0].1.correlation_id(), Some(1));
        let response = rmp_serde::from_slice::<PartitionRpcResponse>(sent[0].1.payload()).unwrap();
        assert!(matches!(response, PartitionRpcResponse::ReadKey(None)));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn handle_partition_rpc_without_bus_fails_entity_not_found() {
        unset_current_message_bus();
        let contract = make_contract().await;

        // Pre-insert a row so the execute step succeeds and the bus lookup is reached.
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();
        let insert = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
            value,
        };
        contract.execute_partition_rpc(insert).await.unwrap();

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let payload = rmp_serde::to_vec(&read).unwrap();
        let envelope = Envelope::new(
            1,
            None,
            2,
            contract.worker_id(),
            PARTITION_RPC_REQUEST_KIND,
            payload,
            DeliveryMode::Request,
        );
        let result = contract.handle_partition_rpc(envelope).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().ec(), ErrorCode::EntityNotFound);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn build_cross_partition_tx_ops_groups_writes_uses_default_worker() {
        let data_dir = temp_dir()
            .join(format!("rpc_build_ops_{}", gen_oid()))
            .to_string_lossy()
            .to_string();
        let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr: Arc::new(TestMetaMgr::new()),
            log: None,
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id: 7,
            default_unpartitioned_worker_id: 7,
            partition_id: 0,
            data_dir,
            async_runtime: None,
            server_instance_id: 0,
        })
        .unwrap();

        let tx = contract.worker_begin_tx().unwrap();
        tx.put_relation(
            PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            },
            b"k0".to_vec(),
            b"v0".to_vec(),
        );
        tx.put_relation(
            PhysicalRelationId {
                table_id: 2,
                partition_id: 0,
            },
            b"k1".to_vec(),
            b"v1".to_vec(),
        );
        tx.put_relation(
            PhysicalRelationId {
                table_id: 3,
                partition_id: 1,
            },
            b"k2".to_vec(),
            b"v2".to_vec(),
        );
        tx.build_write_ops();

        let (participants, write_set) = contract
            .build_cross_partition_tx_ops(tx.as_ref())
            .await
            .unwrap();
        assert_eq!(write_set.len(), 3);
        let participant_by_partition: BTreeMap<OID, OID> = participants
            .into_iter()
            .map(|p| (p.partition_id, p.worker_id))
            .collect();
        assert_eq!(participant_by_partition.len(), 2);
        assert_eq!(participant_by_partition.get(&0).copied(), Some(7));
        assert_eq!(participant_by_partition.get(&1).copied(), Some(7));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_cross_partition_ops_applies_local_writes() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let participants = vec![CrossPartitionParticipant {
            partition_id: 0,
            worker_id: contract.worker_id(),
        }];
        let write_set = vec![XLWrite::Insert(XLInsert {
            table_id: table_id(),
            partition_id: 0,
            tuple_id: 0,
            key: key.clone(),
            value,
        })];

        contract
            .apply_cross_partition_ops(1, participants, write_set)
            .await
            .unwrap();

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn apply_cross_partition_ops_sends_remote_request() {
        let bus = Arc::new(MockMessageBus::new(10));
        set_current_message_bus(bus.clone());
        let data_dir = temp_dir()
            .join(format!("rpc_remote_apply_{}", gen_oid()))
            .to_string_lossy()
            .to_string();
        let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr: Arc::new(TestMetaMgr::new()),
            log: None,
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id: 10,
            default_unpartitioned_worker_id: 20,
            partition_id: 0,
            data_dir,
            async_runtime: None,
            server_instance_id: 0,
        })
        .unwrap();

        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::ApplyCrossPartitionTx,
        ));

        let participants = vec![CrossPartitionParticipant {
            partition_id: 0,
            worker_id: 20,
        }];
        let write_set = vec![XLWrite::Insert(XLInsert {
            table_id: 1,
            partition_id: 0,
            tuple_id: 0,
            key: b"rk".to_vec(),
            value: b"rv".to_vec(),
        })];

        contract
            .apply_cross_partition_ops(1, participants, write_set.clone())
            .await
            .unwrap();

        let sent = bus.take_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 20);
        let decoded = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
        assert!(matches!(
            decoded,
            PartitionRpcRequest::ApplyCrossPartitionTx { .. }
        ));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn worker_commit_cross_partition_tx_async_commits_local_writes() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key1 = build_key_tuple(&key_row(1), &desc).unwrap();
        let value1 = build_value_tuple(&value_row(10), &desc).unwrap();
        let key2 = build_key_tuple(&key_row(2), &desc).unwrap();
        let value2 = build_value_tuple(&value_row(20), &desc).unwrap();

        let tx = contract.worker_begin_tx().unwrap();
        tx.put_relation(
            PhysicalRelationId {
                table_id: table_id(),
                partition_id: 0,
            },
            key1.clone(),
            value1,
        );
        tx.put_relation(
            PhysicalRelationId {
                table_id: table_id(),
                partition_id: 0,
            },
            key2.clone(),
            value2,
        );

        contract
            .worker_commit_cross_partition_tx_async(tx)
            .await
            .unwrap();

        for (key, expected) in [(key1, 10), (key2, 20)] {
            let read = PartitionRpcRequest::ReadKey {
                table_id: table_id(),
                partition_id: 0,
                key,
                select: vec![1],
            };
            let response = contract.execute_partition_rpc(read).await.unwrap();
            assert_eq!(
                response,
                PartitionRpcResponse::ReadKey(Some(vec![Some(datum(expected))]))
            );
        }
    }

    // 以下 handoff 测试用单进程 mock bus 近似双 worker：协调方 contract
    // （worker 10，非分区表 owner 为 worker 20）与 owner contract（worker 20）
    // 是两个独立实例，handoff 请求从 mock bus 捕获后交给 owner 实例执行。

    #[tokio::test(flavor = "current_thread")]
    async fn handoff_commit_single_remote_owner_applies_on_owner() {
        let bus = Arc::new(MockMessageBus::new(10));
        set_current_message_bus(bus.clone());
        let coordinator = make_contract_with_worker(10, 20).await;
        let owner = make_contract_with_worker(20, 20).await;

        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let tx = coordinator.worker_begin_tx().unwrap();
        tx.put_relation(
            PhysicalRelationId {
                table_id: table_id(),
                partition_id: 0,
            },
            key.clone(),
            value,
        );

        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::CommitWriteSet(1),
        ));
        coordinator.worker_commit_routed_tx_async(tx).await.unwrap();

        // The coordinator sent exactly one CommitWriteSet handoff to worker 20.
        let sent = bus.take_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 20);
        let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
        let PartitionRpcRequest::CommitWriteSet { writes, .. } = request else {
            panic!("expected CommitWriteSet request");
        };
        assert_eq!(writes.len(), 1);

        // The coordinator did not write the row locally.
        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: key.clone(),
            select: vec![1],
        };
        let response = coordinator.execute_partition_rpc(read).await.unwrap();
        assert_eq!(response, PartitionRpcResponse::ReadKey(None));

        // The owner commits the handed-off write set through its normal local
        // commit path and the row becomes visible there.
        let response = owner
            .execute_partition_rpc(PartitionRpcRequest::CommitWriteSet {
                tx_id: 1,
                lock_token: None,
                writes,
            })
            .await
            .unwrap();
        assert_eq!(response, PartitionRpcResponse::CommitWriteSet(1));
        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let response = owner.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
        );
        unset_current_message_bus();
    }

    /// Regression test: a point read whose target partition is owned by
    /// another worker must go to the owner even when this worker is worker 0.
    /// The old `self.worker_id != 0` clause in the remote-branch guard made
    /// worker 0 fall through to a local read against relation files it does
    /// not host, returning an empty result for every remote-partition row.
    #[tokio::test(flavor = "current_thread")]
    async fn worker_zero_read_key_for_remote_owned_partition_goes_remote() {
        let bus = Arc::new(MockMessageBus::new(0));
        set_current_message_bus(bus.clone());
        // This contract IS worker 0; the partition resolves to worker 20.
        let contract = make_contract_with_worker(0, 20).await;

        let tx = contract.begin_tx().await.unwrap();
        bus.push_response(response_envelope(
            1,
            20,
            0,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))])),
        ));
        let row = contract
            .read_key(
                tx.clone(),
                table_id(),
                &key_row(1),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await
            .unwrap();
        assert_eq!(row, Some(vec![Some(datum(10))]));

        // Exactly one ReadKey RPC went to the owner, worker 20.
        let sent = bus.take_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 20);
        let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
        assert!(matches!(request, PartitionRpcRequest::ReadKey { .. }));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn handoff_commit_write_set_conflict_returns_error() {
        let bus = Arc::new(MockMessageBus::new(10));
        set_current_message_bus(bus.clone());
        let coordinator = make_contract_with_worker(10, 20).await;
        let owner = make_contract_with_worker(20, 20).await;

        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        // Simulate a conflicting concurrent commit on the owner by holding the
        // commit lock for the same key from another transaction.
        let write_ops = vec![(
            PhysicalRelationId {
                table_id: table_id(),
                partition_id: 0,
            },
            key.clone(),
        )];
        assert!(owner.tx_lock.try_lock_some(999, &write_ops).unwrap());
        let conflict = owner
            .execute_partition_rpc(PartitionRpcRequest::CommitWriteSet {
                tx_id: 7,
                lock_token: None,
                writes: vec![XLWrite::Insert(XLInsert {
                    table_id: table_id(),
                    partition_id: 0,
                    tuple_id: 0,
                    key: key.clone(),
                    value: value.clone(),
                })],
            })
            .await;
        assert!(conflict.is_err());
        owner.tx_lock.release(999, &write_ops).unwrap();

        // The conflict error travels back to the coordinator through the Err
        // response and fails the routed commit.
        let tx = coordinator.worker_begin_tx().unwrap();
        tx.put_relation(
            PhysicalRelationId {
                table_id: table_id(),
                partition_id: 0,
            },
            key,
            value,
        );
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::Err(conflict.unwrap_err().to_string()),
        ));
        let result = coordinator.worker_commit_routed_tx_async(tx).await;
        assert!(result.is_err());
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_write_staging_read_your_writes() {
        let bus = Arc::new(MockMessageBus::new(10));
        set_current_message_bus(bus.clone());
        let contract = make_contract_with_worker(10, 20).await;

        let tx = contract.begin_tx().await.unwrap();
        // The statement-time existence check goes to the owner and reports the
        // key as missing, so the insert is staged locally.
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::ReadKey(None),
        ));
        contract
            .insert(
                tx.clone(),
                table_id(),
                &key_row(1),
                &value_row(10),
                &OptInsert::default(),
            )
            .await
            .unwrap();

        // Read-your-writes: the staged overlay serves the read, no extra RPC.
        let row = contract
            .read_key(
                tx.clone(),
                table_id(),
                &key_row(1),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await
            .unwrap();
        assert_eq!(row, Some(vec![Some(datum(10))]));
        assert_eq!(bus.take_sent().len(), 1);

        // A staged delete reads back as missing.
        let deleted = contract
            .delete(
                tx.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &OptDelete::default(),
            )
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        let row = contract
            .read_key(
                tx.clone(),
                table_id(),
                &key_row(1),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await
            .unwrap();
        assert_eq!(row, None);
        assert!(bus.take_sent().is_empty());

        // Re-inserting over the staged delete needs no remote existence check.
        contract
            .insert(
                tx.clone(),
                table_id(),
                &key_row(1),
                &value_row(30),
                &OptInsert::default(),
            )
            .await
            .unwrap();
        let row = contract
            .read_key(
                tx.clone(),
                table_id(),
                &key_row(1),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await
            .unwrap();
        assert_eq!(row, Some(vec![Some(datum(30))]));
        assert!(bus.take_sent().is_empty());

        contract.abort_tx(tx).await.unwrap();
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_read_range_merges_staged_overlay() {
        let bus = Arc::new(MockMessageBus::new(10));
        set_current_message_bus(bus.clone());
        let contract = make_contract_with_worker(10, 20).await;

        let tx = contract.begin_tx().await.unwrap();
        // Stage an insert of key 5 (remote existence check: missing).
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::ReadKey(None),
        ));
        contract
            .insert(
                tx.clone(),
                table_id(),
                &key_row(5),
                &value_row(50),
                &OptInsert::default(),
            )
            .await
            .unwrap();
        // Stage a delete of key 3 (remote existence check: present).
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::ReadKey(Some(vec![])),
        ));
        let deleted = contract
            .delete(
                tx.clone(),
                table_id(),
                &key_row(3),
                &Predicate::CNF(vec![]),
                &OptDelete::default(),
            )
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        // The owner still holds keys 1..=4; the select includes the key
        // attribute so the augmented-projection dedup path is exercised.
        let remote_rows: Vec<Vec<Option<Vec<u8>>>> = (1..=4)
            .map(|i| vec![Some(datum(i)), Some(datum(i * 10))])
            .collect();
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::ReadRange(remote_rows),
        ));
        let cursor = contract
            .read_range(
                tx.clone(),
                table_id(),
                &RangeData::new(
                    std::ops::Bound::Included(vec![(0, datum(1))]),
                    std::ops::Bound::Unbounded,
                ),
                &Predicate::CNF(vec![]),
                &VecSelTerm::new(vec![0, 1]),
                &OptRead::default(),
            )
            .await
            .unwrap();

        let mut rows = Vec::new();
        while let Some(row) = cursor.next().await.unwrap() {
            rows.push((row.get(0).unwrap(), row.get(1).unwrap()));
        }
        // Key 3 is deleted by the staged overlay, key 5 is inserted by it.
        assert_eq!(
            rows,
            vec![
                (datum(1), datum(10)),
                (datum(2), datum(20)),
                (datum(4), datum(40)),
                (datum(5), datum(50)),
            ]
        );

        // One existence check per write plus one range scan were sent.
        assert_eq!(bus.take_sent().len(), 3);
        contract.abort_tx(tx).await.unwrap();
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn acquire_commit_locks_retries_until_holder_releases() {
        let mgr = Arc::new(XLockMgr::new());
        let relation = PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        };
        let ops = vec![(relation, b"k".to_vec())];
        assert!(mgr.try_lock_some(999, &ops).unwrap());

        let releaser = mgr.clone();
        let release_ops = ops.clone();
        tokio::spawn(async move {
            let _ = mudu_sys::task::async_::sleep(Duration::from_millis(5)).await;
            releaser.release(999, &release_ops).unwrap();
        });

        acquire_commit_locks(&mgr, 1000, &ops).await.unwrap();
        // The lock is now held by xid 1000; another owner cannot grab it.
        assert!(!mgr.try_lock_some(2000, &ops).unwrap());
        mgr.release(1000, &ops).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn acquire_commit_locks_gives_up_after_bounded_attempts() {
        let mgr = XLockMgr::new();
        let relation = PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        };
        let ops = vec![(relation, b"k".to_vec())];
        assert!(mgr.try_lock_some(999, &ops).unwrap());

        let err = acquire_commit_locks(&mgr, 1000, &ops).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Transaction);
        assert!(err.to_string().contains("failed to acquire commit locks"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routed_commit_all_local_behaves_like_local_commit() {
        let bus = Arc::new(MockMessageBus::new(0));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let key = build_key_tuple(&key_row(1), &desc).unwrap();
        let value = build_value_tuple(&value_row(10), &desc).unwrap();

        let tx = contract.worker_begin_tx().unwrap();
        tx.put_relation(
            PhysicalRelationId {
                table_id: table_id(),
                partition_id: 0,
            },
            key.clone(),
            value,
        );
        contract.worker_commit_routed_tx_async(tx).await.unwrap();

        // A fully local commit sends no RPC at all.
        assert!(bus.take_sent().is_empty());

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
        );
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn statement_lock_serializes_concurrent_updates() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        // Seed key 1 with value 10.
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(10), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        // tx1 takes the statement-level write lock on key 1.
        let tx1 = contract.worker_begin_tx().unwrap();
        contract
            ._update(
                desc.clone(),
                tx1.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum(20))]),
                &[],
            )
            .await
            .unwrap();

        // tx2's update of the same key must park on the statement lock and
        // proceed after tx1 commits — instead of failing at commit time.
        let tx2 = contract.worker_begin_tx().unwrap();
        let tx2_key = key_row(1);
        let tx2_pred = Predicate::CNF(vec![]);
        let tx2_values = VecDatum::new(vec![(1, datum(30))]);
        let tx2_update = contract._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &tx2_key,
            &tx2_pred,
            &tx2_values,
            &[],
        );
        let delayed_commit = async {
            let _ = mudu_sys::task::async_::sleep(Duration::from_millis(20)).await;
            contract.worker_commit_tx_async(tx1).await.unwrap();
        };
        let (tx2_result, ()) = futures::join!(tx2_update, delayed_commit);
        assert_eq!(tx2_result.unwrap(), 1);
        contract.worker_commit_tx_async(tx2).await.unwrap();

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(30))]))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn statement_lock_serializes_concurrent_delta_updates() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        // Seed key 1 with value 10.
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(10), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        // tx1 takes the statement-level write lock on key 1 with `v = v + 1`.
        let tx1 = contract.worker_begin_tx().unwrap();
        let tx1_deltas = [DeltaAssign {
            attr: 1,
            op: DeltaOp::Add,
            literal: datum(1),
        }];
        contract
            ._update(
                desc.clone(),
                tx1.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![]),
                &tx1_deltas,
            )
            .await
            .unwrap();

        // tx2's `v = v + 1` parks on the statement lock; after tx1 commits it
        // must evaluate on the latest committed value (11), not on a stale
        // snapshot, so no increment is lost.
        let tx2 = contract.worker_begin_tx().unwrap();
        let tx2_deltas = [DeltaAssign {
            attr: 1,
            op: DeltaOp::Add,
            literal: datum(1),
        }];
        let tx2_values = VecDatum::new(vec![]);
        let tx2_pred = Predicate::CNF(vec![]);
        let tx2_key = key_row(1);
        let tx2_update = contract._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &tx2_key,
            &tx2_pred,
            &tx2_values,
            &tx2_deltas,
        );
        let delayed_commit = async {
            let _ = mudu_sys::task::async_::sleep(Duration::from_millis(20)).await;
            contract.worker_commit_tx_async(tx1).await.unwrap();
        };
        let (tx2_result, ()) = futures::join!(tx2_update, delayed_commit);
        assert_eq!(tx2_result.unwrap(), 1);
        contract.worker_commit_tx_async(tx2).await.unwrap();

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(12))]))
        );
    }

    /// Two deferred conditional-restock updates must NOT serialize on a
    /// statement lock (the second update returns while the first transaction
    /// is still open), and must still produce the same value as the
    /// commutative form `((current - 10 - q) mod 91) + 10` in either apply
    /// order.
    #[tokio::test(flavor = "current_thread")]
    async fn deferred_sub_wrap_updates_are_lockfree_and_commute() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        // Seed key 1 with stock 50.
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(50), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        let wrap_delta = |quantity: i64| DeltaAssign {
            attr: 1,
            op: DeltaOp::SubWrapDeferred,
            literal: crate::server::x_contract::utils::encode_sub_wrap_literal(quantity, 10, 91),
        };
        // tx1 stages a restock update but stays open; tx2's update must NOT
        // park on a statement lock (lock-free path).
        let tx1 = contract.worker_begin_tx().unwrap();
        contract
            ._update(
                desc.clone(),
                tx1.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![]),
                &[wrap_delta(5)],
            )
            .await
            .unwrap();
        let tx2 = contract.worker_begin_tx().unwrap();
        let tx2_key = key_row(1);
        let tx2_pred = Predicate::CNF(vec![]);
        let tx2_values = VecDatum::new(vec![]);
        let tx2_deltas = [wrap_delta(85)];
        let tx2_update = contract._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &tx2_key,
            &tx2_pred,
            &tx2_values,
            &tx2_deltas,
        );
        let updated = mudu_sys::task::async_::timeout(Duration::from_millis(500), tx2_update)
            .await
            .expect("deferred update must not block on a statement lock")
            .unwrap();
        assert_eq!(updated, 1);

        // Commit tx2 before tx1: apply order is 85-then-5. 50 -> ((50-95) mod
        // 91)+10 = 56, then 56 -> ((56-15) mod 91)+10 = 51. The other order
        // (5 then 85: 50 -> 45 -> 51) yields the same value.
        contract.worker_commit_tx_async(tx2).await.unwrap();
        contract.worker_commit_tx_async(tx1).await.unwrap();
        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(51))]))
        );
    }

    /// Deferred add/sub increments from concurrent transactions all apply on
    /// top of each other (no lost update) without a statement lock, and a
    /// point read inside the transaction observes its own staged deferred
    /// delta (read-your-writes fold).
    #[tokio::test(flavor = "current_thread")]
    async fn deferred_add_accumulates_lockfree_and_reads_your_writes() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(10), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        let add_deferred = |attr, v: i32| DeltaAssign {
            attr,
            op: DeltaOp::AddDeferred,
            literal: datum(v),
        };
        let tx1 = contract.worker_begin_tx().unwrap();
        contract
            ._update(
                desc.clone(),
                tx1.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![]),
                &[add_deferred(1, 1)],
            )
            .await
            .unwrap();
        // Read-your-writes: the staged deferred delta folds over the
        // committed value (10 + 1) without being applied yet.
        let row = contract
            .read_key(
                tx1.clone(),
                table_id(),
                &key_row(1),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await
            .unwrap();
        assert_eq!(row, Some(vec![Some(datum(11))]));

        let tx2 = contract.worker_begin_tx().unwrap();
        let tx2_key = key_row(1);
        let tx2_pred = Predicate::CNF(vec![]);
        let tx2_values = VecDatum::new(vec![]);
        let tx2_deltas = [add_deferred(1, 2)];
        let tx2_update = contract._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &tx2_key,
            &tx2_pred,
            &tx2_values,
            &tx2_deltas,
        );
        mudu_sys::task::async_::timeout(Duration::from_millis(500), tx2_update)
            .await
            .expect("deferred update must not block on a statement lock")
            .unwrap();

        contract.worker_commit_tx_async(tx2).await.unwrap();
        contract.worker_commit_tx_async(tx1).await.unwrap();
        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(13))]))
        );
    }

    /// Deferred deltas cannot be mixed with absolute assignments in one call.
    #[tokio::test(flavor = "current_thread")]
    async fn deferred_delta_mixed_with_absolute_assignment_is_rejected() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(10), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        let tx = contract.worker_begin_tx().unwrap();
        let result = contract
            ._update(
                desc.clone(),
                tx.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &value_row(99),
                &[DeltaAssign {
                    attr: 1,
                    op: DeltaOp::AddDeferred,
                    literal: datum(1),
                }],
            )
            .await;
        assert!(result.is_err());
    }

    /// The conditional-restock evaluation is exactly `((x - 10 - q) mod 91) + 10`:
    /// threshold boundary respected, and any two restock updates commute.
    #[test]
    fn sub_wrap_eval_commutes_and_respects_threshold() {
        let desc = meta_table(&test_schema()).unwrap();
        let eval = |current: i32, q: i64| -> i32 {
            let updated = apply_value_update_with_deltas(
                &build_value_tuple(&value_row(current), &desc).unwrap(),
                &VecDatum::new(vec![]),
                &[DeltaAssign {
                    attr: 1,
                    op: DeltaOp::SubWrapDeferred,
                    literal: crate::server::x_contract::utils::encode_sub_wrap_literal(q, 10, 91),
                }],
                &desc,
            )
            .unwrap();
            let field = desc.value_desc().get_field_desc(0).get(&updated).unwrap();
            i32::from_be_bytes(field.try_into().unwrap())
        };
        // Boundary: 50 - 40 = 10 is NOT below 10, so no restock wrap.
        assert_eq!(eval(50, 40), 10);
        // 50 - 41 = 9 < 10 triggers the wrap: 9 + 91 = 100.
        assert_eq!(eval(50, 41), 100);
        // f5(f85(x)) == f85(f5(x)) for the boundary-crossing case too.
        assert_eq!(eval(eval(50, 85), 5), eval(eval(50, 5), 85));
        // wrap <= floor is rejected.
        let rejected = apply_value_update_with_deltas(
            &build_value_tuple(&value_row(50), &desc).unwrap(),
            &VecDatum::new(vec![]),
            &[DeltaAssign {
                attr: 1,
                op: DeltaOp::SubWrapDeferred,
                literal: crate::server::x_contract::utils::encode_sub_wrap_literal(5, 10, 10),
            }],
            &desc,
        );
        assert!(rejected.is_err());
    }

    /// Bind `v = v + ?` through the real SQL binder with the amount supplied
    /// as a parameter, returning the resulting delta assignment.
    async fn bind_parameterized_delta(sql: &str, params: &(i32, i32)) -> DeltaAssign {
        let meta_mgr = Arc::new(TestMetaMgr::new());
        crate::contract::meta_mgr::MetaMgr::create_table(meta_mgr.as_ref(), &test_schema())
            .await
            .unwrap();
        let binder = crate::sql::binder::Binder::new(meta_mgr);
        let stmt = sql_parser::ast::parser::SQLParser::new()
            .unwrap()
            .parse(sql)
            .unwrap()
            .stmts()[0]
            .clone();
        let bound = binder.bind(stmt, params).await.unwrap();
        let crate::sql::bound_stmt::BoundStmt::Command(
            crate::sql::bound_stmt::BoundCommand::Update(update),
        ) = bound
        else {
            panic!("expected bound update");
        };
        assert_eq!(update.value.len(), 1);
        let crate::sql::bound_stmt::BoundSetValue::Delta { op, literal } = &update.value[0].1
        else {
            panic!("expected delta assignment, got {:?}", update.value[0].1);
        };
        DeltaAssign {
            attr: update.value[0].0,
            op: *op,
            literal: literal.clone(),
        }
    }

    // Miri cannot execute FFI calls into the tree-sitter C parser used by
    // SQLParser inside `bind_parameterized_delta`; skipped under Miri.
    #[tokio::test(flavor = "current_thread")]
    #[cfg_attr(miri, ignore)]
    async fn statement_lock_serializes_concurrent_parameterized_delta_updates() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        // Seed key 1 with value 100.
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(100), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        // Two transactions each run a payment-style `v = v + ?` with amount 7
        // supplied as a parameter.
        let tx1_deltas =
            [
                bind_parameterized_delta("update t set v = v + ? where id = ?;", &(7i32, 1i32))
                    .await,
            ];
        let tx1 = contract.worker_begin_tx().unwrap();
        contract
            ._update(
                desc.clone(),
                tx1.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![]),
                &tx1_deltas,
            )
            .await
            .unwrap();

        // tx2's parameterized delta parks on the statement lock; after tx1
        // commits it must evaluate on the latest committed value (107), so the
        // final value is 100 + 2 * 7 and no increment is lost.
        let tx2_deltas =
            [
                bind_parameterized_delta("update t set v = v + ? where id = ?;", &(7i32, 1i32))
                    .await,
            ];
        let tx2 = contract.worker_begin_tx().unwrap();
        let tx2_key = key_row(1);
        let tx2_pred = Predicate::CNF(vec![]);
        let tx2_values = VecDatum::new(vec![]);
        let tx2_update = contract._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &tx2_key,
            &tx2_pred,
            &tx2_values,
            &tx2_deltas,
        );
        let delayed_commit = async {
            let _ = mudu_sys::task::async_::sleep(Duration::from_millis(20)).await;
            contract.worker_commit_tx_async(tx1).await.unwrap();
        };
        let (tx2_result, ()) = futures::join!(tx2_update, delayed_commit);
        assert_eq!(tx2_result.unwrap(), 1);
        contract.worker_commit_tx_async(tx2).await.unwrap();

        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(114))]))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_applies_mixed_absolute_and_delta_assignments() {
        let contract = make_contract().await;
        // A table with three value columns so one UPDATE can mix an absolute
        // assignment with multiple delta assignments, like TPC-C payment.
        let schema = crate::contract::schema_table::SchemaTable::new_with_oid(
            43,
            "t2".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "a".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "b".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "c".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
            ],
            vec![0],
            vec![1, 2, 3],
        );
        let desc = meta_table(&schema).unwrap();
        let tx = contract.begin_tx().await.unwrap();
        contract.create_table(tx.clone(), &schema).await.unwrap();
        contract.commit_tx(tx).await.unwrap();

        // Seed key 1 with (a, b, c) = (1, 100, 1000).
        let seed = PartitionRpcRequest::Insert {
            table_id: schema.id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(
                &VecDatum::new(vec![(1, datum(1)), (2, datum(100)), (3, datum(1000))]),
                &desc,
            )
            .unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        // One statement: a = 7 (absolute), b = b + 5 (delta), c = c - 30
        // (delta).
        let tx = contract.worker_begin_tx().unwrap();
        let updated = contract
            ._update(
                desc.clone(),
                tx.clone(),
                schema.id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum(7))]),
                &[
                    DeltaAssign {
                        attr: 2,
                        op: DeltaOp::Add,
                        literal: datum(5),
                    },
                    DeltaAssign {
                        attr: 3,
                        op: DeltaOp::Sub,
                        literal: datum(30),
                    },
                ],
            )
            .await
            .unwrap();
        assert_eq!(updated, 1);
        contract.worker_commit_tx_async(tx).await.unwrap();

        let read = PartitionRpcRequest::ReadKey {
            table_id: schema.id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            select: vec![1, 2, 3],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![
                Some(datum(7)),
                Some(datum(105)),
                Some(datum(970))
            ]))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn statement_lock_cross_update_deadlock_resolves_by_timeout() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        for (k, v) in [(1, 10), (2, 20)] {
            let seed = PartitionRpcRequest::Insert {
                table_id: table_id(),
                partition_id: 0,
                key: build_key_tuple(&key_row(k), &desc).unwrap(),
                value: build_value_tuple(&value_row(v), &desc).unwrap(),
            };
            contract.execute_partition_rpc(seed).await.unwrap();
        }

        // tx1 locks k1, tx2 locks k2; each then tries the other's key.
        let tx1 = contract.worker_begin_tx().unwrap();
        contract
            ._update(
                desc.clone(),
                tx1.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum(11))]),
                &[],
            )
            .await
            .unwrap();
        let tx2 = contract.worker_begin_tx().unwrap();
        contract
            ._update(
                desc.clone(),
                tx2.clone(),
                table_id(),
                &key_row(2),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum(21))]),
                &[],
            )
            .await
            .unwrap();

        let wait1_key = key_row(2);
        let wait1_pred = Predicate::CNF(vec![]);
        let wait1_values = VecDatum::new(vec![(1, datum(22))]);
        let wait1 = contract._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &wait1_key,
            &wait1_pred,
            &wait1_values,
            &[],
        );
        let wait2_key = key_row(1);
        let wait2_pred = Predicate::CNF(vec![]);
        let wait2_values = VecDatum::new(vec![(1, datum(12))]);
        let wait2 = contract._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &wait2_key,
            &wait2_pred,
            &wait2_values,
            &[],
        );
        let started = *mudu_sys::time::instant_now();
        let (r1, r2) = futures::join!(wait1, wait2);
        // The circular wait is broken by the bounded statement-lock wait
        // (STATEMENT_LOCK_TIMEOUT), never by a hang; at least one side
        // reports the Transaction error.
        assert!(started.elapsed() < STATEMENT_LOCK_TIMEOUT * 2);
        assert!(
            r1.as_ref()
                .err()
                .is_some_and(|e| e.ec() == ErrorCode::Transaction)
                || r2
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.ec() == ErrorCode::Transaction),
            "expected a transaction timeout on one side: {r1:?} {r2:?}"
        );
        let _ = contract.worker_abort_tx_async(tx1).await;
        let _ = contract.worker_abort_tx_async(tx2).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn statement_lock_released_on_rollback() {
        let contract = make_contract().await;
        let desc = meta_table(&test_schema()).unwrap();
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(10), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();

        let tx1 = contract.worker_begin_tx().unwrap();
        contract
            ._update(
                desc.clone(),
                tx1.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum(20))]),
                &[],
            )
            .await
            .unwrap();
        contract.worker_abort_tx_async(tx1).await.unwrap();

        // The rollback released the statement lock: tx2 takes it without
        // waiting (far below the 25ms timeout).
        let tx2 = contract.worker_begin_tx().unwrap();
        let acquired = mudu_sys::task::async_::timeout(
            Duration::from_millis(500),
            contract._update(
                desc.clone(),
                tx2.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum(30))]),
                &[],
            ),
        )
        .await
        .expect("update after rollback must not park");
        assert_eq!(acquired.unwrap(), 1);
        let _ = contract.worker_abort_tx_async(tx2).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_read_range_key_prefix_eq_filters_rows() {
        let bus = Arc::new(MockMessageBus::new(10));
        set_current_message_bus(bus.clone());
        let contract = make_contract_with_worker(10, 20).await;

        let tx = contract.begin_tx().await.unwrap();
        // The owner returns rows with the augmented projection (key attrs
        // first); the key-prefix predicate must filter them client-side.
        let remote_rows: Vec<Vec<Option<Vec<u8>>>> = (1..=3)
            .map(|i| vec![Some(datum(i)), Some(datum(i * 10))])
            .collect();
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::ReadRange(remote_rows),
        ));
        let cursor = contract
            .read_range(
                tx.clone(),
                table_id(),
                &RangeData::new(
                    std::ops::Bound::Included(vec![(0, datum(1))]),
                    std::ops::Bound::Unbounded,
                ),
                &Predicate::KeyPrefixEq(vec![(0, datum(1))]),
                &VecSelTerm::new(vec![0, 1]),
                &OptRead::default(),
            )
            .await
            .unwrap();

        let mut rows = Vec::new();
        while let Some(row) = cursor.next().await.unwrap() {
            rows.push((row.get(0).unwrap(), row.get(1).unwrap()));
        }
        assert_eq!(rows, vec![(datum(1), datum(10))]);
        contract.abort_tx(tx).await.unwrap();
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_statement_lock_and_handoff_commit_full_lifecycle() {
        let bus = Arc::new(MockMessageBus::new(10));
        set_current_message_bus(bus.clone());
        let coordinator = make_contract_with_worker(10, 20).await;
        let owner = make_contract_with_worker(20, 20).await;
        let desc = meta_table(&test_schema()).unwrap();

        // Seed the row on the owner.
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(1), &desc).unwrap(),
            value: build_value_tuple(&value_row(10), &desc).unwrap(),
        };
        owner.execute_partition_rpc(seed).await.unwrap();

        // The coordinator's update sends LockKeyForUpdate; pre-queue the
        // projected value the owner would return.
        let tx = coordinator.worker_begin_tx().unwrap();
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))])),
        ));
        coordinator
            ._update(
                desc.clone(),
                tx.clone(),
                table_id(),
                &key_row(1),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum(30))]),
                &[],
            )
            .await
            .unwrap();
        assert_eq!(tx.remote_lock_owners(), vec![20]);

        // The coordinator locked via LockKeyForUpdate carrying its statement
        // token; replay that request against the real owner so its lock
        // table reflects the lock.
        let sent = bus.take_sent();
        assert_eq!(sent.len(), 1);
        let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
        let token = statement_lock_token(10, tx.xid());
        let PartitionRpcRequest::LockKeyForUpdate {
            lock_token,
            table_id: req_table,
            partition_id: req_partition,
            key: req_key,
            ..
        } = request
        else {
            panic!("expected LockKeyForUpdate request");
        };
        assert_eq!(lock_token, token);
        let relation_id = PhysicalRelationId {
            table_id: req_table,
            partition_id: req_partition,
        };
        let owner_response = owner
            .execute_partition_rpc(PartitionRpcRequest::LockKeyForUpdate {
                lock_token,
                table_id: req_table,
                partition_id: req_partition,
                key: req_key.clone(),
                select: vec![1],
            })
            .await
            .unwrap();
        assert_eq!(
            owner_response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
        );
        // The owner now holds the token's lock on the key.
        assert!(!owner
            .tx_lock
            .try_lock_some(999, &[(relation_id, req_key.clone())])
            .unwrap());

        // Handoff commit carries the token; the owner commits under the
        // held lock and releases all of the token's locks afterwards.
        bus.push_response(response_envelope(
            1,
            20,
            10,
            PartitionRpcResponse::CommitWriteSet(1),
        ));
        coordinator.worker_commit_routed_tx_async(tx).await.unwrap();
        let sent = bus.take_sent();
        assert_eq!(sent.len(), 1);
        let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
        let PartitionRpcRequest::CommitWriteSet {
            lock_token: commit_token,
            writes,
            ..
        } = request
        else {
            panic!("expected CommitWriteSet request");
        };
        assert_eq!(commit_token, Some(token));
        let response = owner
            .execute_partition_rpc(PartitionRpcRequest::CommitWriteSet {
                tx_id: 1,
                lock_token: commit_token,
                writes,
            })
            .await
            .unwrap();
        assert_eq!(response, PartitionRpcResponse::CommitWriteSet(1));

        // Committed value visible on the owner, and the token's locks are
        // all released.
        assert!(owner
            .tx_lock
            .try_lock_some(999, &[(relation_id, req_key.clone())])
            .unwrap());
        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key: req_key,
            select: vec![1],
        };
        let response = owner.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(30))]))
        );
        unset_current_message_bus();
    }
}
