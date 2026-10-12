#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{block_on, datum, key_row, meta_table, read_i32_value, test_schema, value_row};
use crate::utils::{build_key_tuple, build_value_tuple};
use crate::*;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
use ::wal::worker_log::decode_frames;
use mudu_sys::env_var::temp_dir;

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
    let txm = WorkerTxManager::new(::catalog::contract::worker_snapshot::WorkerSnapshot::new(
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
    let decoded = ::wal::xl_batch::decode_xl_batches(&frames).unwrap();
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
    let batch = XLBatch::new(vec![::wal::xl_entry::XLEntry {
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
                value: build_value_tuple(&value_row(30), &meta_table(&schema).unwrap()).unwrap(),
            })),
            TxOp::Commit(Default::default()),
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
                key: build_key_tuple(&key_row(8), &meta_table(&test_schema()).unwrap()).unwrap(),
                value: build_value_tuple(&value_row(80), &meta_table(&test_schema()).unwrap())
                    .unwrap(),
            })),
            TxOp::Commit(Default::default()),
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
    let batch = XLBatch::new(vec![::wal::xl_entry::XLEntry {
        xid: 7,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(::wal::xl_data_op::XLWrite::Delete(
                ::wal::xl_data_op::XLDelete {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"wk".to_vec(),
                },
            )),
            TxOp::Commit(Default::default()),
        ],
    }]);

    contract.replay_worker_log_batch(batch).await.unwrap();

    assert_eq!(contract.worker_get_async(b"wk").await.unwrap(), None);
}

#[test]
fn group_commit_tx_committed_row_replays_from_wal() {
    block_on(async move {
        let r = _group_commit_tx_committed_row_replays_from_wal().await;
        if let Err(e) = &r {
            panic!("group commit replay failed: {}", e);
        }
    })
}

async fn _group_commit_tx_committed_row_replays_from_wal() -> RS<()> {
    let dir = temp_dir().join(format!("group_commit_replay_{}", gen_oid()));
    let layout = WorkerLogLayout::new(dir, gen_oid(), 4096)?;
    let log = ChunkedWorkerLogBackend::new(layout.clone()).await?;
    let meta_mgr = Arc::new(TestMetaMgr::new());
    let schema = test_schema();
    let table_id = schema.id();
    let contract = WorkerXContract::with_log(meta_mgr.clone(), Some(log))?;
    contract.initialize().await?;
    let ddl_tx = contract.begin_tx().await?;
    contract.create_table(ddl_tx.clone(), &schema).await?;
    contract.commit_tx(ddl_tx).await?;
    let insert_tx = contract.begin_tx().await?;
    let insert_xid = insert_tx.xid();
    contract
        .insert(
            insert_tx.clone(),
            table_id,
            &key_row(1),
            &value_row(10),
            &OptInsert::default(),
        )
        .await?;
    // The commit goes through the group-commit queue and self-drives the
    // flush round, so the batch must be durable when commit_tx returns.
    contract.commit_tx(insert_tx).await?;

    // Simulate crash recovery: replay the persisted WAL batch into a
    // fresh contract that already has the table (DDL recovery is
    // meta-manager driven, not batch driven).
    let bytes = mudu_sys::fs::sync::read(layout.chunk_path(0))?;
    let frames = decode_frames(&bytes)?;
    let batches = ::wal::xl_batch::decode_xl_batches(&frames)?;
    let recovered = WorkerXContract::with_log(meta_mgr, None)?;
    recovered.initialize().await?;
    let ddl_tx = recovered.begin_tx().await?;
    recovered.create_table(ddl_tx.clone(), &schema).await?;
    recovered.commit_tx(ddl_tx).await?;
    let mut replayed = 0;
    for batch in batches {
        if batch.entries.iter().any(|entry| entry.xid == insert_xid) {
            recovered.replay_worker_log_batch(batch).await?;
            replayed += 1;
        }
    }
    assert_eq!(replayed, 1);
    let row = read_i32_value(&recovered, table_id, 1).await?;
    assert_eq!(row, Some(datum(10)));
    Ok(())
}
