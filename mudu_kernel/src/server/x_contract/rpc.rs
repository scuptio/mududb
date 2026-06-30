use super::utils::*;
use super::*;

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
        }
    }

    async fn send_partition_rpc(
        &self,
        target_worker_id: OID,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
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
    ) -> RS<Option<Vec<Option<DatBin>>>> {
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
    ) -> RS<Vec<Vec<Option<DatBin>>>> {
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

    pub(crate) async fn remote_insert(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::Insert {
                    table_id,
                    partition_id,
                    key,
                    value,
                },
            )
            .await?
        {
            PartitionRpcResponse::Insert => Ok(()),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected insert rpc response"
            )),
        }
    }

    pub(crate) async fn remote_delete(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
    ) -> RS<usize> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::Delete {
                    table_id,
                    partition_id,
                    key,
                },
            )
            .await?
        {
            PartitionRpcResponse::Delete(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected delete rpc response"
            )),
        }
    }

    pub(crate) async fn remote_update(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        values: Vec<(AttrIndex, Vec<u8>)>,
    ) -> RS<usize> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::Update {
                    table_id,
                    partition_id,
                    key,
                    values,
                },
            )
            .await?
        {
            PartitionRpcResponse::Update(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected update rpc response"
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

    pub(crate) async fn worker_commit_cross_partition_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
    ) -> RS<()> {
        let xid = tx.xid();
        tx.build_write_ops();
        let write_ops = tx.write_ops();
        let can_commit = self.tx_lock.try_lock_some(xid as OID, &write_ops)?;
        if !can_commit {
            return Err(mudu_error!(
                ErrorCode::Transaction,
                format!("transaction {} failed to acquire commit locks", xid)
            ));
        }

        let result = async {
            let _prepared = self.storage.prepare_commit_async(tx.as_ref()).await?;
            let (participants, write_set) = self.build_cross_partition_tx_ops(tx.as_ref()).await?;
            if let Some(log) = self.log_cloned()? {
                let batch = XLBatch::new(vec![XLEntry {
                    xid,
                    ops: cross_partition_wal_ops(&write_set),
                }]);
                new_xl_batch_writer(log.clone()).append(&batch).await?;
                log.flush_async().await?;
            }
            self.apply_cross_partition_ops(xid as OID, participants, write_set)
                .await
        }
        .await;

        self.tx_lock.release(xid as OID, &write_ops)?;
        self.worker_rollback_tx(tx)?;
        result
    }

    async fn build_cross_partition_tx_ops(
        &self,
        tx: &dyn TxMgr,
    ) -> RS<(Vec<CrossPartitionParticipant>, Vec<XLWrite>)> {
        let mut participants = BTreeMap::new();
        let mut write_set = Vec::new();
        for (relation_id, rows) in tx.staged_relation_ops() {
            let worker_id = self
                .resolve_partition_worker(relation_id.partition_id)
                .await?
                .unwrap_or(self.worker_id);
            participants.insert(relation_id.partition_id, worker_id);
            for (key, value) in rows {
                match value {
                    Some(value) => write_set.push(XLWrite::Insert(XLInsert {
                        table_id: relation_id.table_id,
                        partition_id: relation_id.partition_id,
                        tuple_id: 0,
                        key,
                        value,
                    })),
                    None => write_set.push(XLWrite::Delete(XLDelete {
                        table_id: relation_id.table_id,
                        partition_id: relation_id.partition_id,
                        tuple_id: 0,
                        key,
                    })),
                }
            }
        }
        Ok((
            participants
                .into_iter()
                .map(|(partition_id, worker_id)| CrossPartitionParticipant {
                    partition_id,
                    worker_id,
                })
                .collect(),
            write_set,
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
            if participant.worker_id != 0
                && self.worker_id != 0
                && participant.worker_id != self.worker_id
            {
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
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dt_fn_param::DatType;
    use mudu_type::dt_info::DTInfo;
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
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
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
        let data_dir = temp_dir()
            .join(format!("rpc_test_{}", gen_oid()))
            .to_string_lossy()
            .to_string();
        let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr: Arc::new(TestMetaMgr::new()),
            log: None,
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id: 0,
            default_unpartitioned_worker_id: 0,
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
    async fn remote_insert_matches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Insert));

        contract
            .remote_insert(2, table_id(), 0, b"k".to_vec(), b"v".to_vec())
            .await
            .unwrap();
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_insert_mismatches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Delete(0)));

        let result = contract
            .remote_insert(2, table_id(), 0, b"k".to_vec(), b"v".to_vec())
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unexpected insert rpc response"));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_delete_matches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Delete(1)));

        let result = contract
            .remote_delete(2, table_id(), 0, b"k".to_vec())
            .await
            .unwrap();
        assert_eq!(result, 1);
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_delete_mismatches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Insert));

        let result = contract
            .remote_delete(2, table_id(), 0, b"k".to_vec())
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unexpected delete rpc response"));
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_update_matches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Update(1)));

        let result = contract
            .remote_update(2, table_id(), 0, b"k".to_vec(), vec![(1, b"v".to_vec())])
            .await
            .unwrap();
        assert_eq!(result, 1);
        unset_current_message_bus();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_update_mismatches_variant() {
        let bus = Arc::new(MockMessageBus::new(1));
        set_current_message_bus(bus.clone());
        let contract = make_contract().await;
        bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Insert));

        let result = contract
            .remote_update(2, table_id(), 0, b"k".to_vec(), vec![(1, b"v".to_vec())])
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unexpected update rpc response"));
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
}
