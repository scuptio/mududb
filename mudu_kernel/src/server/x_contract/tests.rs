#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::super::utils::{build_key_tuple, build_value_tuple};
    use super::super::*;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::table_info::TableInfo;
    use crate::server::test_meta_mgr::TestMetaMgr;
    use crate::wal::worker_log::{decode_frames, ChunkedWorkerLogBackend, WorkerLogLayout};
    use crate::wal::xl_data_op::XLInsert;
    use crate::wal::xl_entry::TxOp;
    use mudu_sys::env_var::temp_dir;
    use mudu_type::data_type_fn_param::DataType;
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::type_family::TypeFamily;
    use mudu_utils::oid::gen_oid;
    use std::future::Future;

    fn block_on<F>(fut: F) -> F::Output
    where
        F: Future,
    {
        mudu_sys::task::async_::build_current_thread_runtime()
            .unwrap()
            .block_on(fut)
    }

    fn test_schema() -> SchemaTable {
        SchemaTable::new(
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        )
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

    fn datum_string(v: &str) -> Vec<u8> {
        mudu_type::data_type_function::send_binary(
            &mudu_type::data_value::DataValue::from_string(v.to_string()),
            &mudu_type::data_type::DataType::default_for(
                mudu_type::type_family::TypeFamily::String,
            ),
        )
        .unwrap()
    }

    fn wallet_users_schema() -> SchemaTable {
        use crate::contract::schema_column::SchemaColumn;
        use mudu_type::data_type_info::DataTypeInfo;

        SchemaTable::new(
            "users".to_string(),
            vec![
                SchemaColumn::new(
                    "user_id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    TypeFamily::String,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
                ),
                SchemaColumn::new(
                    "phone".to_string(),
                    TypeFamily::String,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
                ),
                SchemaColumn::new(
                    "email".to_string(),
                    TypeFamily::String,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
                ),
                SchemaColumn::new(
                    "password".to_string(),
                    TypeFamily::String,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
                ),
                SchemaColumn::new(
                    "created_at".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "updated_at".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
            ],
            vec![0],
            vec![1, 2, 3, 4, 5, 6],
        )
    }

    #[test]
    fn relation_commit_log_round_trips() {
        block_on(async move {
            let r = _relation_commit_log_round_trips().await;
            assert!(r.is_ok())
        })
    }

    async fn _relation_commit_log_round_trips() -> RS<()> {
        let mgr = Arc::new(TestMetaMgr::new());
        let storage = WorkerStorage::new(
            mgr.clone(),
            0,
            mudu_sys::env_var::temp_dir()
                .join(format!("xcontract_relation_log_{}", gen_oid()))
                .to_string_lossy()
                .to_string(),
        );
        let schema = test_schema();
        let table_id = schema.id();
        storage.create_table_async(&schema).await?;
        let txm = WorkerTxManager::new(crate::server::worker_snapshot::WorkerSnapshot::new(
            9,
            vec![],
        ));
        storage
            .put(table_id, b"k1".to_vec(), b"v1".to_vec(), &txm)
            .await?;
        storage.remove(table_id, b"k1", &txm).await?;
        let prepared = storage.prepare_commit_async(&txm).await?;

        assert_eq!(prepared.batch().entries.len(), 1);
        assert_eq!(prepared.batch().entries[0].xid, 9);
        assert!(matches!(prepared.batch().entries[0].ops[0], TxOp::Begin));
        Ok(())
    }

    #[test]
    fn iouring_xcontract_commit_persists_relation_log() {
        block_on(async move {
            let r = _iouring_xcontract_commit_persists_relation_log().await;
            assert!(r.is_ok())
        })
    }

    async fn _iouring_xcontract_commit_persists_relation_log() -> RS<()> {
        let dir = temp_dir().join(format!("iouring_xcontract_log_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096)?;
        let log = ChunkedWorkerLogBackend::new(layout.clone()).await?;
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, Some(log))?;
        contract.initialize().await?;
        let ddl_tx = contract.begin_tx().await?;
        contract.create_table(ddl_tx.clone(), &schema).await?;
        contract.commit_tx(ddl_tx).await?;
        let tx_mgr = contract.begin_tx().await?;
        let keys = key_row(1);
        let values = value_row(10);
        let opt_insert = OptInsert::default();
        contract
            .insert(tx_mgr.clone(), table_id, &keys, &values, &opt_insert)
            .await?;
        contract.commit_tx(tx_mgr).await?;

        let bytes = mudu_sys::fs::sync::read(layout.chunk_path(0)).unwrap();
        let frames = decode_frames(&bytes).unwrap();
        let decoded = crate::wal::xl_batch::decode_xl_batches(&frames).unwrap();
        assert_eq!(decoded.len(), 1);
        let insert = decoded[0].entries[0]
            .ops
            .iter()
            .find_map(|op| match op {
                TxOp::Write(XLWrite::Insert(insert)) => Some(insert),
                _ => None,
            })
            .unwrap();
        assert_eq!(insert.table_id, table_id);
        assert_eq!(
            insert.key,
            build_key_tuple(&key_row(1), &meta_table(&schema).unwrap())?
        );
        let desc = meta_table(&schema)?;
        let tuple = build_value_tuple(&value_row(10), desc.as_ref())?;
        assert_eq!(insert.value, tuple);
        Ok(())
    }

    #[test]
    fn iouring_xcontract_replay_restores_worker_kv_and_relation_rows() {
        block_on(async move {
            let r = _iouring_xcontract_replay_restores_worker_kv_and_relation_rows().await;
            assert!(r.is_ok())
        })
    }

    async fn _iouring_xcontract_replay_restores_worker_kv_and_relation_rows() -> RS<()> {
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

        let tx_mgr = contract.begin_tx().await?;
        contract.create_table(tx_mgr.clone(), &schema).await?;
        contract.commit_tx(tx_mgr).await?;
        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 11,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"wk".to_vec(),
                    value: b"wv".to_vec(),
                })),
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id,
                    partition_id: 0,
                    tuple_id: 0,
                    key: build_key_tuple(&key_row(3), &meta_table(&schema).unwrap()).unwrap(),
                    value: build_value_tuple(&value_row(30), &meta_table(&schema).unwrap())
                        .unwrap(),
                })),
                TxOp::Commit,
            ],
        }]);

        contract.replay_worker_log_batch(batch).await.unwrap();

        assert_eq!(
            contract.worker_get_async(b"wk").await.unwrap(),
            Some(b"wv".to_vec())
        );

        let xid = contract.begin_tx().await?;
        let pred_key = key_row(3);
        let select = VecSelTerm::new(vec![1]);
        let opt_read = OptRead::default();
        let relation = contract
            .read_key(xid, table_id, &pred_key, &select, &opt_read)
            .await?;
        assert_eq!(relation, Some(vec![Some(datum(30))]));
        Ok(())
    }

    #[test]
    fn cross_partition_recovery_is_coordinator_driven() {
        block_on(async move {
            let r = _cross_partition_recovery_is_coordinator_driven().await;
            assert!(r.is_ok())
        })
    }

    async fn _cross_partition_recovery_is_coordinator_driven() -> RS<()> {
        let worker_id = gen_oid();
        let (contract, table_id) = {
            let meta_mgr = Arc::new(TestMetaMgr::new());
            let schema = test_schema();
            let table_id = schema.id();
            let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
                meta_mgr,
                log: None,
                log_layout: Default::default(),
                active_sessions: Default::default(),
                worker_id,
                default_unpartitioned_worker_id: worker_id,
                partition_id: 0,
                data_dir: temp_dir()
                    .join(format!("cross_partition_recovery_{}", gen_oid()))
                    .to_string_lossy()
                    .to_string(),
                async_runtime: None,
                server_instance_id: 0,
            })?;
            let ddl_tx = contract.begin_tx().await?;
            contract.create_table(ddl_tx.clone(), &schema).await?;
            contract.commit_tx(ddl_tx).await?;
            (contract, table_id)
        };

        let batch = XLBatch::new(vec![XLEntry {
            xid: 88,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id,
                    partition_id: 0,
                    tuple_id: 0,
                    key: build_key_tuple(&key_row(8), &meta_table(&test_schema()).unwrap())
                        .unwrap(),
                    value: build_value_tuple(&value_row(80), &meta_table(&test_schema()).unwrap())
                        .unwrap(),
                })),
                TxOp::Commit,
            ],
        }]);

        contract.replay_worker_log_batch(batch).await?;
        let before_finish = read_i32_value(&contract, table_id, 8).await?;
        assert_eq!(before_finish, Some(datum(80)));

        contract.finish_worker_log_recovery()?;
        let after_finish = read_i32_value(&contract, table_id, 8).await?;
        assert_eq!(after_finish, Some(datum(80)));
        Ok(())
    }

    async fn read_i32_value(
        contract: &WorkerXContract,
        table_id: OID,
        key: i32,
    ) -> RS<Option<Vec<u8>>> {
        let tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                tx.clone(),
                table_id,
                &key_row(key),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await?;
        contract.abort_tx(tx).await?;
        Ok(row.and_then(|mut row| row.pop().flatten()))
    }

    #[test]
    fn xcontract_insert_and_read_nullable_value_column() {
        block_on(async move {
            let meta_mgr = Arc::new(TestMetaMgr::new());
            let schema = test_schema();
            let table_id = schema.id();
            let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

            let ddl = contract.begin_tx().await?;
            contract.create_table(ddl.clone(), &schema).await?;
            contract.commit_tx(ddl).await?;

            let tx = contract.begin_tx().await?;
            contract
                .insert(
                    tx.clone(),
                    table_id,
                    &key_row(7),
                    &VecDatum::new(Vec::new()),
                    &OptInsert::default(),
                )
                .await?;
            contract.commit_tx(tx).await?;

            let read_tx = contract.begin_tx().await?;
            let row = contract
                .read_key(
                    read_tx,
                    table_id,
                    &key_row(7),
                    &VecSelTerm::new(vec![1]),
                    &OptRead::default(),
                )
                .await?;
            assert_eq!(row, Some(vec![None]));
            Ok::<(), mudu::error::MuduError>(())
        })
        .unwrap();
    }

    #[test]
    fn build_value_tuple_supports_partial_insert_with_mixed_types() {
        let schema = wallet_users_schema();
        let desc = meta_table(&schema).unwrap();
        let input = VecDatum::new(vec![
            (1, datum_string("Alice")),
            (2, datum_string("12345678")),
            (3, datum_string("alice@xxx.com")),
            (4, datum_string("aaa")),
            (5, datum(0)),
        ]);
        let tuple = build_value_tuple(&input, &desc).unwrap();
        assert!(!tuple.is_empty());
    }

    #[test]
    fn iouring_xcontract_replay_applies_worker_kv_delete() {
        block_on(async move { _iouring_xcontract_replay_applies_worker_kv_delete().await })
    }

    async fn _iouring_xcontract_replay_applies_worker_kv_delete() {
        let contract = WorkerXContract::with_worker_log(
            ChunkedWorkerLogBackend::new(
                WorkerLogLayout::new(
                    temp_dir().join(format!("iouring_xcontract_worker_log_{}", gen_oid())),
                    gen_oid(),
                    4096,
                )
                .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await
        .unwrap();

        contract
            .worker_put_async(b"wk".to_vec(), b"wv".to_vec())
            .await
            .unwrap();
        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 7,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(crate::wal::xl_data_op::XLWrite::Delete(
                    crate::wal::xl_data_op::XLDelete {
                        table_id: 0,
                        partition_id: 0,
                        tuple_id: 0,
                        key: b"wk".to_vec(),
                    },
                )),
                TxOp::Commit,
            ],
        }]);

        contract.replay_worker_log_batch(batch).await.unwrap();

        assert_eq!(contract.worker_get_async(b"wk").await.unwrap(), None);
    }

    #[test]
    fn iouring_xcontract_update_maps_table_attr_to_value_tuple_index() {
        block_on(async move {
            let r = _iouring_xcontract_update_maps_table_attr_to_value_tuple_index().await;
            assert!(r.is_ok())
        })
    }

    async fn _iouring_xcontract_update_maps_table_attr_to_value_tuple_index() -> RS<()> {
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

        let ddl_tx = contract.begin_tx().await?;
        contract.create_table(ddl_tx.clone(), &schema).await?;
        contract.commit_tx(ddl_tx).await?;

        let insert_tx = contract.begin_tx().await?;
        let insert_key = key_row(1);
        let insert_value = value_row(10);
        let opt_insert = OptInsert::default();
        contract
            .insert(
                insert_tx.clone(),
                table_id,
                &insert_key,
                &insert_value,
                &opt_insert,
            )
            .await?;
        contract.commit_tx(insert_tx).await?;

        let update_tx = contract.begin_tx().await?;
        let update_key = key_row(1);
        let pred_non_key = Predicate::CNF(vec![]);
        let update_value = value_row(20);
        let updated = contract
            .update(
                update_tx.clone(),
                table_id,
                &update_key,
                &pred_non_key,
                &update_value,
                &OptUpdate {},
            )
            .await?;
        assert_eq!(updated, 1);
        contract.commit_tx(update_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let read_key = key_row(1);
        let select = VecSelTerm::new(vec![1]);
        let opt_read = OptRead::default();
        let relation = contract
            .read_key(read_tx, table_id, &read_key, &select, &opt_read)
            .await?;
        assert_eq!(relation, Some(vec![Some(datum(20))]));
        Ok(())
    }

    fn meta_table(schema: &SchemaTable) -> RS<Arc<TableDesc>> {
        TableInfo::new(schema.clone())?.table_desc()
    }

    async fn make_contract_with_table(schema: &SchemaTable) -> RS<(Arc<WorkerXContract>, OID)> {
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, None)?;
        let ddl_tx = contract.begin_tx().await?;
        contract.create_table(ddl_tx.clone(), schema).await?;
        contract.commit_tx(ddl_tx).await?;
        Ok((Arc::new(contract), table_id))
    }

    #[test]
    fn xcontract_delete_removes_existing_row() {
        block_on(async move {
            let schema = test_schema();
            let (contract, table_id) = make_contract_with_table(&schema).await?;

            let insert_tx = contract.begin_tx().await?;
            contract
                .insert(
                    insert_tx.clone(),
                    table_id,
                    &key_row(1),
                    &value_row(10),
                    &OptInsert::default(),
                )
                .await?;
            contract.commit_tx(insert_tx).await?;

            let delete_tx = contract.begin_tx().await?;
            let deleted = contract
                .delete(
                    delete_tx.clone(),
                    table_id,
                    &key_row(1),
                    &Predicate::CNF(Vec::new()),
                    &OptDelete::default(),
                )
                .await?;
            assert_eq!(deleted, 1);
            contract.commit_tx(delete_tx).await?;

            let read_tx = contract.begin_tx().await?;
            let row = contract
                .read_key(
                    read_tx,
                    table_id,
                    &key_row(1),
                    &VecSelTerm::new(vec![1]),
                    &OptRead::default(),
                )
                .await?;
            assert_eq!(row, None);
            Ok::<(), mudu::error::MuduError>(())
        })
        .unwrap();
    }

    #[test]
    fn xcontract_delete_missing_row_returns_zero() {
        block_on(async move {
            let schema = test_schema();
            let (contract, table_id) = make_contract_with_table(&schema).await?;

            let tx = contract.begin_tx().await?;
            let deleted = contract
                .delete(
                    tx.clone(),
                    table_id,
                    &key_row(99),
                    &Predicate::CNF(Vec::new()),
                    &OptDelete::default(),
                )
                .await?;
            assert_eq!(deleted, 0);
            contract.abort_tx(tx).await?;
            Ok::<(), mudu::error::MuduError>(())
        })
        .unwrap();
    }

    #[test]
    fn xcontract_read_range_returns_matching_rows() {
        block_on(async move {
            let schema = test_schema();
            let (contract, table_id) = make_contract_with_table(&schema).await?;

            let insert_tx = contract.begin_tx().await?;
            for (k, v) in [(1, 10), (2, 20), (3, 30)] {
                contract
                    .insert(
                        insert_tx.clone(),
                        table_id,
                        &key_row(k),
                        &value_row(v),
                        &OptInsert::default(),
                    )
                    .await?;
            }
            contract.commit_tx(insert_tx).await?;

            let read_tx = contract.begin_tx().await?;
            let cursor = contract
                .read_range(
                    read_tx.clone(),
                    table_id,
                    &RangeData::new(
                        std::ops::Bound::Included(vec![(0, datum(1))]),
                        std::ops::Bound::Included(vec![(0, datum(3))]),
                    ),
                    &Predicate::CNF(Vec::new()),
                    &VecSelTerm::new(vec![1]),
                    &OptRead::default(),
                )
                .await?;

            let mut rows = Vec::new();
            while let Some(row) = cursor.next().await? {
                rows.push(row);
            }
            assert_eq!(rows.len(), 3);
            contract.abort_tx(read_tx).await?;
            Ok::<(), mudu::error::MuduError>(())
        })
        .unwrap();
    }

    #[test]
    fn xcontract_read_range_with_key_prefix_eq_filters_rows() {
        block_on(async move {
            let schema = test_schema();
            let (contract, table_id) = make_contract_with_table(&schema).await?;

            let insert_tx = contract.begin_tx().await?;
            for (k, v) in [(1, 10), (2, 20), (3, 30)] {
                contract
                    .insert(
                        insert_tx.clone(),
                        table_id,
                        &key_row(k),
                        &value_row(v),
                        &OptInsert::default(),
                    )
                    .await?;
            }
            contract.commit_tx(insert_tx).await?;

            let read_tx = contract.begin_tx().await?;
            let cursor = contract
                .read_range(
                    read_tx.clone(),
                    table_id,
                    &RangeData::new(
                        std::ops::Bound::Included(vec![(0, datum(1))]),
                        std::ops::Bound::Included(vec![(0, datum(3))]),
                    ),
                    &Predicate::KeyPrefixEq(vec![(0, datum(2))]),
                    &VecSelTerm::new(vec![1]),
                    &OptRead::default(),
                )
                .await?;

            let mut rows = Vec::new();
            while let Some(row) = cursor.next().await? {
                rows.push(row);
            }
            assert_eq!(rows.len(), 1);
            contract.abort_tx(read_tx).await?;
            Ok::<(), mudu::error::MuduError>(())
        })
        .unwrap();
    }
}
