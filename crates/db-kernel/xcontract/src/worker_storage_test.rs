#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::future::Future;

use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
use mudu::common::id::OID;
use mudu_sys::common::provider_type::ProviderType;
use mudu_sys::provider::create_io_provider;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;

use super::*;

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

async fn test_storage() -> RS<(WorkerStorage, OID)> {
    let mgr = Arc::new(TestMetaMgr::new());
    let storage = WorkerStorage::new(
        mgr,
        0,
        mudu_sys::env_var::temp_dir()
            .join(format!(
                "worker_storage_test_{}",
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .to_string(),
    );
    let schema = test_schema();
    let oid = schema.id();
    storage.create_table_async(&schema).await?;
    Ok((storage, oid))
}

async fn test_shared_storage() -> RS<(
    Arc<TestMetaMgr>,
    Arc<WorkerStorage>,
    Arc<WorkerStorage>,
    OID,
)> {
    let mgr = Arc::new(TestMetaMgr::new());
    let root = mudu_sys::env_var::temp_dir()
        .join(format!(
            "worker_storage_shared_test_{}",
            mudu_utils::oid::gen_oid()
        ))
        .to_string_lossy()
        .to_string();
    let storage1 = Arc::new(WorkerStorage::new(mgr.clone(), 1, root.clone()));
    storage1.register_global()?;
    storage1.bootstrap_existing_tables_async().await?;
    let storage2 = Arc::new(WorkerStorage::new(mgr.clone(), 2, root));
    storage2.register_global()?;
    storage2.bootstrap_existing_tables_async().await?;

    let schema = test_schema();
    let oid = schema.id();
    storage1.create_table_async(&schema).await?;
    Ok((mgr, storage1, storage2, oid))
}

fn begin_tx(xid: u64, running: Vec<u64>) -> WorkerTxManager {
    WorkerTxManager::new(WorkerSnapshot::new(xid, running))
}

fn i32_bytes(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

#[test]
fn worker_storage_broadcasts_create_and_drop_to_peer_workers() {
    block_on(async move {
        let r = _worker_storage_broadcasts_create_and_drop_to_peer_workers().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_broadcasts_create_and_drop_to_peer_workers() -> RS<()> {
    let (mgr, _storage1, storage2, oid) = test_shared_storage().await?;
    let mut tx = begin_tx(1, vec![]);
    storage2.put(oid, i32_bytes(7), i32_bytes(70), &tx).await?;
    storage2.commit_tx(&mut tx).await?;
    assert!(mgr.get_table_by_id(oid).await.is_ok());

    storage2.drop_table_async(oid).await?;
    assert!(mgr.get_table_by_id(oid).await.is_err());

    let tx = begin_tx(2, vec![]);
    let err = storage2
        .put(oid, i32_bytes(8), i32_bytes(80), &tx)
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("no such table"));
    Ok(())
}

#[test]
fn worker_storage_bootstraps_existing_tables_with_async_runtime() {
    block_on(async move {
        let r = _worker_storage_bootstraps_existing_tables_with_async_runtime().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_bootstraps_existing_tables_with_async_runtime() -> RS<()> {
    let mgr = Arc::new(TestMetaMgr::new());
    let schema = test_schema();
    let oid = schema.id();
    mgr.create_table(&schema).await?;
    let storage = WorkerStorage::new_with_async_runtime(
        mgr,
        0,
        mudu_sys::env_var::temp_dir()
            .join(format!(
                "worker_storage_async_bootstrap_test_{}",
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .to_string(),
        Some(create_io_provider(ProviderType::Tokio)),
    );
    storage.bootstrap_existing_tables_async().await?;

    let mut tx = begin_tx(1, vec![]);
    storage.put(oid, i32_bytes(1), i32_bytes(10), &tx).await?;
    storage.commit_tx(&mut tx).await?;
    let read_tx = begin_tx(2, vec![]);
    assert_eq!(
        storage.get(oid, &i32_bytes(1), &read_tx).await?,
        Some(i32_bytes(10))
    );
    Ok(())
}

#[test]
fn worker_storage_reads_own_writes() {
    block_on(async move {
        let r = _worker_storage_reads_own_writes().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_reads_own_writes() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let tx = begin_tx(10, vec![]);

    storage.put(oid, i32_bytes(1), i32_bytes(11), &tx).await?;

    assert_eq!(
        storage.get(oid, &i32_bytes(1), &tx).await?,
        Some(i32_bytes(11))
    );

    let contain_key = storage
        .contains_key(oid, &KeyTuple::from(i32_bytes(1)), &tx)
        .await?;
    assert!(contain_key);
    Ok(())
}

#[test]
fn worker_storage_read_committed_sees_later_commit() {
    block_on(async move {
        let r = _worker_storage_read_committed_sees_later_commit().await;
        assert!(r.is_ok())
    })
}

// Relation reads are READ COMMITTED: a later statement of an open
// transaction observes versions committed after the transaction began.
async fn _worker_storage_read_committed_sees_later_commit() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let mut tx1 = begin_tx(1, vec![]);
    storage.put(oid, i32_bytes(1), i32_bytes(10), &tx1).await?;
    storage.commit_tx(&mut tx1).await?;

    let old_tx = begin_tx(2, vec![]);
    let mut new_tx = begin_tx(3, vec![2]);
    storage
        .put(oid, i32_bytes(1), i32_bytes(20), &new_tx)
        .await?;
    storage.commit_tx(&mut new_tx).await?;

    assert_eq!(
        storage.get(oid, &i32_bytes(1), &old_tx).await?,
        Some(i32_bytes(20))
    );
    Ok(())
}

#[test]
fn worker_storage_range_is_read_committed() {
    block_on(async move {
        let r = _worker_storage_range_is_read_committed().await;
        assert!(r.is_ok())
    })
}

// Range reads are READ COMMITTED too: rows committed after the
// transaction began appear in later range scans of that transaction.
async fn _worker_storage_range_is_read_committed() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let mut seed = begin_tx(1, vec![]);
    storage.put(oid, i32_bytes(1), i32_bytes(10), &seed).await?;
    storage.commit_tx(&mut seed).await?;

    let old_tx = begin_tx(2, vec![]);
    let mut new_tx = begin_tx(3, vec![2]);
    storage
        .put(oid, i32_bytes(2), i32_bytes(20), &new_tx)
        .await?;
    storage.commit_tx(&mut new_tx).await?;

    let rows = storage
        .range(
            oid,
            (
                Included(i32_bytes(1).as_slice()),
                Included(i32_bytes(9).as_slice()),
            ),
            &old_tx,
        )
        .await?;
    assert_eq!(
        rows,
        vec![(i32_bytes(1), i32_bytes(10)), (i32_bytes(2), i32_bytes(20))]
    );
    Ok(())
}

#[test]
fn worker_storage_first_committer_wins() {
    block_on(async move {
        let r = _worker_storage_first_committer_wins().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_first_committer_wins() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let mut seed = begin_tx(1, vec![]);
    storage.put(oid, i32_bytes(1), i32_bytes(10), &seed).await?;
    storage.commit_tx(&mut seed).await?;

    let mut tx1 = begin_tx(2, vec![]);
    let mut tx2 = begin_tx(3, vec![2]);
    storage.put(oid, i32_bytes(1), i32_bytes(11), &tx1).await?;
    storage.put(oid, i32_bytes(1), i32_bytes(12), &tx2).await?;
    storage.commit_tx(&mut tx1).await?;
    let err = storage.commit_tx(&mut tx2).await.unwrap_err();

    assert!(err.to_string().contains("write-write conflict"));
    Ok(())
}

#[test]
fn worker_storage_delete_visible_after_commit_read_committed() {
    block_on(async move {
        let r = _worker_storage_delete_visible_after_commit_read_committed().await;
        assert!(r.is_ok())
    })
}

// READ COMMITTED: a committed delete is visible to later statements of
// any transaction, including one that began before the delete.
async fn _worker_storage_delete_visible_after_commit_read_committed() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let mut seed = begin_tx(1, vec![]);
    storage.put(oid, i32_bytes(1), i32_bytes(10), &seed).await?;
    storage.commit_tx(&mut seed).await?;

    let old_tx = begin_tx(2, vec![]);
    let mut delete_tx = begin_tx(3, vec![2]);
    assert_eq!(
        storage.remove(oid, &i32_bytes(1), &delete_tx).await?,
        Some(i32_bytes(10))
    );
    storage.commit_tx(&mut delete_tx).await?;

    assert_eq!(storage.get(oid, &i32_bytes(1), &old_tx).await?, None);
    let fresh_tx = begin_tx(4, vec![]);
    assert_eq!(storage.get(oid, &i32_bytes(1), &fresh_tx).await?, None);
    Ok(())
}

#[test]
fn worker_storage_kv_snapshot_hides_later_commit() {
    block_on(async move {
        let r = _worker_storage_kv_snapshot_hides_later_commit().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_kv_snapshot_hides_later_commit() -> RS<()> {
    let (storage, _oid) = test_storage().await?;
    storage.worker_put_local(b"a".to_vec(), b"0".to_vec(), 1)?;

    let snapshot = WorkerSnapshot::new(2, vec![]);
    let prepared = storage.prepare_worker_kv_autocommit(
        3,
        b"a".to_vec(),
        Some(b"1".to_vec()),
        XLBatch::new(vec![]),
    );
    storage.apply_prepared_commit_async(prepared).await?;

    assert_eq!(
        storage.kv_get(b"a", Some(&snapshot)).await?,
        Some(b"0".to_vec())
    );
    assert_eq!(storage.kv_get(b"a", None).await?, Some(b"1".to_vec()));
    Ok(())
}

#[test]
fn worker_storage_kv_range_is_stable_with_snapshot() {
    block_on(async move {
        let r = _worker_storage_kv_range_is_stable_with_snapshot().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_kv_range_is_stable_with_snapshot() -> RS<()> {
    let (storage, _oid) = test_storage().await?;
    storage.worker_put_local(b"a".to_vec(), b"1".to_vec(), 1)?;
    let snapshot = WorkerSnapshot::new(2, vec![]);
    storage.worker_put_local(b"b".to_vec(), b"2".to_vec(), 3)?;

    let rows = storage.kv_range(b"a", b"z", Some(&snapshot)).await?;
    assert_eq!(
        rows,
        vec![KvItem {
            key: b"a".to_vec(),
            value: b"1".to_vec()
        }]
    );
    Ok(())
}

#[test]
fn worker_storage_kv_allows_concurrent_commits_on_different_keys() {
    block_on(async move {
        let r = _worker_storage_kv_allows_concurrent_commits_on_different_keys().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_kv_allows_concurrent_commits_on_different_keys() -> RS<()> {
    let (storage, _oid) = test_storage().await?;
    let snapshot1 = WorkerSnapshot::new(1, vec![]);
    let snapshot2 = WorkerSnapshot::new(2, vec![1]);

    let prepared1 = storage
        .prepare_worker_kv_commit(
            &snapshot1,
            snapshot1.xid(),
            BTreeMap::from([(b"a".to_vec(), Some(b"1".to_vec()))]),
            XLBatch::new(vec![]),
        )
        .await?;
    let prepared2 = storage
        .prepare_worker_kv_commit(
            &snapshot2,
            snapshot2.xid(),
            BTreeMap::from([(b"b".to_vec(), Some(b"2".to_vec()))]),
            XLBatch::new(vec![]),
        )
        .await?;

    storage.apply_prepared_commit_async(prepared1).await?;
    storage.apply_prepared_commit_async(prepared2).await?;

    assert_eq!(storage.kv_get(b"a", None).await?, Some(b"1".to_vec()));
    assert_eq!(storage.kv_get(b"b", None).await?, Some(b"2".to_vec()));
    Ok(())
}

#[test]
fn worker_storage_replay_batch_restores_kv_and_relation_rows() {
    block_on(async move {
        let r = _worker_storage_replay_batch_restores_kv_and_relation_rows().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_replay_batch_restores_kv_and_relation_rows() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let batch = XLBatch::new(vec![::wal::xl_entry::XLEntry {
        xid: 9,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            })),
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: oid,
                partition_id: 0,
                tuple_id: 0,
                key: i32_bytes(7),
                value: i32_bytes(70),
            })),
            TxOp::Commit(Default::default()),
        ],
    }]);

    storage.replay_batch(batch).await?;

    assert_eq!(storage.kv_get(b"k", None).await?, Some(b"v".to_vec()));
    let tx = begin_tx(10, vec![]);
    assert_eq!(
        storage.get(oid, &i32_bytes(7), &tx).await?,
        Some(i32_bytes(70))
    );
    Ok(())
}

#[test]
fn worker_storage_replay_batch_applies_kv_delete() {
    block_on(async move {
        let r = _worker_storage_replay_batch_applies_kv_delete().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_replay_batch_applies_kv_delete() -> RS<()> {
    let (storage, _oid) = test_storage().await?;
    storage.worker_put_local(b"k".to_vec(), b"v".to_vec(), 1)?;

    let batch = XLBatch::new(vec![::wal::xl_entry::XLEntry {
        xid: 2,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Delete(XLDelete {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: b"k".to_vec(),
            })),
            TxOp::Commit(Default::default()),
        ],
    }]);

    storage.replay_batch(batch).await?;

    assert_eq!(storage.kv_get(b"k", None).await?, None);
    Ok(())
}

#[test]
fn worker_storage_cross_partition_apply_is_idempotent() {
    block_on(async move {
        let r = _worker_storage_cross_partition_apply_is_idempotent().await;
        assert!(r.is_ok())
    })
}

#[test]
fn worker_storage_cross_partition_apply_covers_all_partitions_of_one_tx() {
    block_on(async move {
        let r = _worker_storage_cross_partition_apply_covers_all_partitions_of_one_tx().await;
        assert!(r.is_ok())
    })
}

/// Deferred conditional-restock replay: an `XLWrite::Update` entry is
/// re-evaluated against the seeded row exactly like the live deferred
/// apply (50 - 41 = 9 < 10 -> 100).
#[test]
fn worker_storage_replay_applies_deferred_delta_update() {
    block_on(async move {
        let r = _worker_storage_replay_applies_deferred_delta_update().await;
        r.unwrap()
    })
}

async fn _worker_storage_replay_applies_deferred_delta_update() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let desc = ::catalog::contract::table_info::TableInfo::new(test_schema())?.table_desc()?;
    let value_of = |v: i32| {
        ::storage::tuple_build::build_value_tuple(
            &::xengine::api::VecDatum::new(vec![(1, i32_bytes(v))]),
            &desc,
        )
        .unwrap()
    };
    let mut seed_tx = begin_tx(1, vec![]);
    storage
        .put(oid, i32_bytes(7), value_of(50), &seed_tx)
        .await?;
    storage.commit_tx(&mut seed_tx).await?;

    let deltas = vec![::xengine::api::DeltaAssign {
        attr: 1,
        op: ::xengine::api::DeltaOp::SubWrapDeferred,
        literal: ::storage::value_update::encode_sub_wrap_literal(41, 10, 91),
    }];
    let batch = XLBatch::new(vec![::wal::xl_entry::XLEntry {
        xid: 2,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Update(XLUpdate {
                table_id: oid,
                partition_id: 0,
                tuple_id: 0,
                key: i32_bytes(7),
                delta: ::storage::tuple_build::encode_delta_assigns(&deltas)?,
            })),
            TxOp::Commit(Default::default()),
        ],
    }]);
    storage.replay_batch(batch).await?;

    let tx = begin_tx(3, vec![]);
    assert_eq!(
        storage.get(oid, &i32_bytes(7), &tx).await?,
        Some(value_of(100))
    );
    Ok(())
}

/// Regression test: one cross-partition transaction whose write set
/// spans two partitions hosted on this worker must persist BOTH rows.
/// The coordinator applies one call per participant partition, so a
/// tx_id-only idempotency guard used to skip every partition after the
/// first on the same worker (silent row loss with full affected_rows).
async fn _worker_storage_cross_partition_apply_covers_all_partitions_of_one_tx() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let write_p1 = XLWrite::Insert(XLInsert {
        table_id: oid,
        partition_id: 1,
        tuple_id: 0,
        key: i32_bytes(1),
        value: i32_bytes(10),
    });
    let write_p2 = XLWrite::Insert(XLInsert {
        table_id: oid,
        partition_id: 2,
        tuple_id: 0,
        key: i32_bytes(2),
        value: i32_bytes(20),
    });

    storage
        .apply_cross_partition_tx_async(77, std::slice::from_ref(&write_p1))
        .await?;
    storage
        .apply_cross_partition_tx_async(77, std::slice::from_ref(&write_p2))
        .await?;

    let tx = begin_tx(78, vec![]);
    assert_eq!(
        storage
            .get_on_partition(oid, Some(1), &i32_bytes(1), &tx)
            .await?,
        Some(i32_bytes(10))
    );
    assert_eq!(
        storage
            .get_on_partition(oid, Some(2), &i32_bytes(2), &tx)
            .await?,
        Some(i32_bytes(20))
    );
    Ok(())
}

async fn _worker_storage_cross_partition_apply_is_idempotent() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let write = XLWrite::Insert(XLInsert {
        table_id: oid,
        partition_id: 0,
        tuple_id: 0,
        key: i32_bytes(9),
        value: i32_bytes(90),
    });

    storage
        .apply_cross_partition_tx_async(77, std::slice::from_ref(&write))
        .await?;
    storage.apply_cross_partition_tx_async(77, &[write]).await?;

    let tx = begin_tx(78, vec![]);
    assert_eq!(
        storage.get(oid, &i32_bytes(9), &tx).await?,
        Some(i32_bytes(90))
    );
    Ok(())
}

#[test]
fn worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables() {
    block_on(async move {
        let r = _worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables().await;
        assert!(r.is_ok())
    })
}

async fn _worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables() -> RS<()> {
    let mgr = Arc::new(TestMetaMgr::new());
    let schema = test_schema();
    let oid = schema.id();
    mgr.create_table(&schema).await?;

    let storage = WorkerStorage::new(
        mgr,
        123,
        mudu_sys::env_var::temp_dir()
            .join(format!(
                "worker_storage_bootstrap_test_{}",
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .to_string(),
    );
    storage.bootstrap_existing_tables_async().await?;

    let batch = XLBatch::new(vec![::wal::xl_entry::XLEntry {
        xid: 11,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: oid,
                partition_id: 0,
                tuple_id: 0,
                key: i32_bytes(5),
                value: i32_bytes(50),
            })),
            TxOp::Commit(Default::default()),
        ],
    }]);

    storage.replay_batch(batch).await?;

    let tx = begin_tx(12, vec![]);
    assert_eq!(
        storage
            .get_on_partition(oid, Some(0), &i32_bytes(5), &tx)
            .await?,
        Some(i32_bytes(50))
    );
    Ok(())
}

#[test]
fn worker_storage_concurrent_first_touch_creates_single_relation() {
    block_on(async move {
        let r = _worker_storage_concurrent_first_touch_creates_single_relation().await;
        assert!(r.is_ok())
    })
}

/// Smoke test for the relation-creation race: many tasks lazily create
/// and write the same (table, partition) relation at once. The creation
/// lock must serialize the opens so every task ends up using the single
/// stored instance.
async fn _worker_storage_concurrent_first_touch_creates_single_relation() -> RS<()> {
    let (storage, oid) = test_storage().await?;
    let storage = Arc::new(storage);
    let partition_id = 9;

    let mut handles = Vec::new();
    for index in 0..8i32 {
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            storage
                .ensure_relation_index(oid, Some(partition_id))
                .await?;
            let mut tx = begin_tx(1 + index as u64, vec![]);
            storage
                .put_on_partition(
                    oid,
                    Some(partition_id),
                    i32_bytes(index),
                    i32_bytes(index * 10),
                    &tx,
                )
                .await?;
            storage.commit_tx(&mut tx).await?;
            Ok::<_, mudu::error::MuduError>(())
        }));
    }
    for handle in handles {
        handle
            .await
            .map_err(|e| mudu_error!(ErrorCode::Tokio, "join first-touch task error", e))??;
    }

    let relation_id = storage.relation_id(oid, partition_id);
    assert!(storage.relation_store.contains_async(&relation_id).await);
    let tx = begin_tx(100, vec![]);
    assert_eq!(
        storage
            .get_on_partition(oid, Some(partition_id), &i32_bytes(3), &tx)
            .await?,
        Some(i32_bytes(30))
    );
    Ok(())
}
