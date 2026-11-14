#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use mudu_sys::env_var::temp_dir;

use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;

use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;

use super::*;

pub(super) fn test_schema() -> SchemaTable {
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

pub(super) fn relation_path() -> String {
    temp_dir()
        .join(format!("relation_rebuild_{}", mudu_utils::oid::gen_oid()))
        .to_string_lossy()
        .to_string()
}

pub(super) fn i32_bytes(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

#[test]
fn rebuilds_index_and_next_tuple_id_from_relation_files() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = test_schema();
        let table_desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();
        let table_id = schema.id();
        let partition_id = 7;
        let path = relation_path();

        let relation = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
            .await
            .unwrap();
        relation
            .write_value(i32_bytes(1), i32_bytes(11), 1)
            .await
            .unwrap();
        relation.write_delete(i32_bytes(1), 2).await.unwrap();
        relation
            .write_value(i32_bytes(2), i32_bytes(22), 3)
            .await
            .unwrap();
        relation.flush_wal_async().await.unwrap();
        drop(relation);

        let reopened = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
            .await
            .unwrap();
        assert_eq!(
            reopened
                .visible_value(
                    &KeyTuple::from(i32_bytes(1)),
                    &WorkerSnapshot::new(1, vec![])
                )
                .await
                .unwrap(),
            Some(i32_bytes(11))
        );
        assert_eq!(
            reopened
                .visible_value(
                    &KeyTuple::from(i32_bytes(1)),
                    &WorkerSnapshot::new(2, vec![])
                )
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            reopened
                .visible_value(
                    &KeyTuple::from(i32_bytes(2)),
                    &WorkerSnapshot::new(3, vec![])
                )
                .await
                .unwrap(),
            Some(i32_bytes(22))
        );

        reopened
            .write_value(i32_bytes(3), i32_bytes(33), 4)
            .await
            .unwrap();
        // Page writes are deferred (WAL-first dirty pages): flush the
        // relation before reading the raw key file without WAL replay.
        reopened.flush_dirty_pages().await.unwrap();
        let key_file = TimeSeriesFile::open_ts_file_sync(
            TimeSeriesFile::relation_file_path(&path, partition_id, table_id, 0),
            false,
        )
        .await
        .unwrap();
        let rows = key_file.scan_range(0, u64::MAX).await.unwrap();
        let k3_row = rows
            .into_iter()
            .find(|row| row.timestamp == 4 && row.payload == i32_bytes(3))
            .unwrap();
        assert_eq!(k3_row.tuple_id, 3);
    })
    .unwrap()
}

#[test]
fn concurrent_readers_and_writer_stay_consistent() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = test_schema();
        let table_desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();
        let relation = std::rc::Rc::new(
            Relation::new(schema.id(), 0, relation_path(), table_desc.as_ref())
                .await
                .unwrap(),
        );
        relation
            .write_value(i32_bytes(1), i32_bytes(10), 1)
            .await
            .unwrap();

        // Readers with the latest snapshot must always observe a
        // well-formed committed value (never a torn or missing payload)
        // while a writer adds new versions and keys.
        let mut readers = Vec::new();
        for _ in 0..4 {
            let relation = relation.clone();
            readers.push(tokio::task::spawn_local(async move {
                for _ in 0..50 {
                    let value = relation
                        .visible_value(
                            &KeyTuple::from(i32_bytes(1)),
                            &WorkerSnapshot::new(1_000_000, vec![]),
                        )
                        .await
                        .unwrap()
                        .expect("committed key must stay visible");
                    assert!(
                        value == i32_bytes(10) || value == i32_bytes(20),
                        "unexpected value {value:?}"
                    );
                    let _ = relation
                        .has_write_conflict(
                            &KeyTuple::from(i32_bytes(1)),
                            &WorkerSnapshot::new(1_000_000, vec![]),
                        )
                        .await
                        .unwrap();
                }
            }));
        }
        let writer = {
            let relation = relation.clone();
            tokio::task::spawn_local(async move {
                for i in 0..50 {
                    relation
                        .write_value(i32_bytes(100 + i), i32_bytes(i), 2 + i as u64)
                        .await
                        .unwrap();
                }
                relation
                    .write_value(i32_bytes(1), i32_bytes(20), 100)
                    .await
                    .unwrap();
            })
        };
        for reader in readers {
            reader.await.unwrap();
        }
        writer.await.unwrap();

        assert_eq!(
            relation
                .visible_value(
                    &KeyTuple::from(i32_bytes(1)),
                    &WorkerSnapshot::new(1_000_000, vec![])
                )
                .await
                .unwrap(),
            Some(i32_bytes(20))
        );
        // Snapshot visibility is intact: an old snapshot still reads the
        // old committed version.
        assert_eq!(
            relation
                .visible_value(
                    &KeyTuple::from(i32_bytes(1)),
                    &WorkerSnapshot::new(50, vec![])
                )
                .await
                .unwrap(),
            Some(i32_bytes(10))
        );
    })
    .unwrap()
}

#[test]
fn write_rows_batch_commit_recovers_on_reopen() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = test_schema();
        let table_desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();
        let table_id = schema.id();
        let partition_id = 9;
        let path = relation_path();

        let relation = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
            .await
            .unwrap();
        // One commit's rows applied as a single batch: enough keys to
        // cross a page boundary inside the key/value files, plus a
        // delete mixed in.
        let mut rows: Vec<(Vec<u8>, Option<Vec<u8>>)> = (0..40)
            .map(|i| (i32_bytes(i), Some(i32_bytes(1000 + i))))
            .collect();
        rows.push((i32_bytes(100), None));
        relation.write_rows(&rows, 5).await.unwrap();
        // A later commit updates some of the same keys through the
        // single-row path.
        relation
            .write_value(i32_bytes(3), i32_bytes(3333), 7)
            .await
            .unwrap();
        // Drain the queued PL frames before the simulated crash (see
        // TimeSeriesFile::flush_wal_async).
        relation.flush_wal_async().await.unwrap();
        drop(relation);

        let reopened = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
            .await
            .unwrap();
        let snapshot = WorkerSnapshot::new(5, vec![]);
        for i in 0..40 {
            assert_eq!(
                reopened
                    .visible_value(&KeyTuple::from(i32_bytes(i)), &snapshot)
                    .await
                    .unwrap(),
                Some(i32_bytes(1000 + i)),
                "key {i} must survive reopen"
            );
        }
        // The batched delete is visible at its own snapshot.
        assert_eq!(
            reopened
                .visible_value(&KeyTuple::from(i32_bytes(100)), &snapshot)
                .await
                .unwrap(),
            None
        );
        // The later single-row update wins at a newer snapshot.
        assert_eq!(
            reopened
                .visible_value(
                    &KeyTuple::from(i32_bytes(3)),
                    &WorkerSnapshot::new(7, vec![])
                )
                .await
                .unwrap(),
            Some(i32_bytes(3333))
        );
    })
    .unwrap()
}

async fn assert_version_payloads(relation: &Relation, reason: &str) {
    let key = KeyTuple::from(i32_bytes(1));
    for xid in 1..=6u64 {
        assert_eq!(
            relation
                .visible_value(&key, &WorkerSnapshot::new(xid, vec![]))
                .await
                .unwrap(),
            Some(i32_bytes(100 + xid as i32)),
            "version {xid} {reason}"
        );
    }
}

async fn assert_range_reads_split_in_memory_and_file(relation: &Relation) {
    let snapshot = |xid: u64| WorkerSnapshot::new(xid, vec![]);
    let key_bytes = i32_bytes(1);
    let latest = relation
        .visible_range(
            (Bound::Included(key_bytes.as_slice()), Bound::Unbounded),
            &snapshot(8),
        )
        .await
        .unwrap();
    assert_eq!(latest, vec![(i32_bytes(1), i32_bytes(200))]);
    let older = relation
        .visible_range(
            (Bound::Included(key_bytes.as_slice()), Bound::Unbounded),
            &snapshot(2),
        )
        .await
        .unwrap();
    assert_eq!(older, vec![(i32_bytes(1), i32_bytes(102))]);
}

#[test]
fn visible_value_serves_in_memory_payload_and_file_fallback() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = test_schema();
        let table_desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();
        let table_id = schema.id();
        let path = relation_path();
        let key = || KeyTuple::from(i32_bytes(1));
        let snapshot = |xid: u64| WorkerSnapshot::new(xid, vec![]);

        let relation = Relation::new(table_id, 13, path.clone(), table_desc.as_ref())
            .await
            .unwrap();
        // Six versions of one key: versions 1..=3 leave the retained
        // four-version window (a delete follows), so reading them
        // exercises the delta-chain + value-file fallback while newer
        // versions are served from the in-memory payload.
        for xid in 1..=6u64 {
            relation
                .write_value(i32_bytes(1), i32_bytes(100 + xid as i32), xid)
                .await
                .unwrap();
        }
        assert_version_payloads(&relation, "must read back its own payload").await;

        // Delete hides the row at its own snapshot but not older ones.
        relation.write_delete(i32_bytes(1), 7).await.unwrap();
        assert_eq!(
            relation.visible_value(&key(), &snapshot(7)).await.unwrap(),
            None
        );
        assert_eq!(
            relation.visible_value(&key(), &snapshot(6)).await.unwrap(),
            Some(i32_bytes(106))
        );

        // Reopen: versions rebuild metadata-only from the files, and
        // every snapshot must still observe the exact committed bytes.
        relation.flush_wal_async().await.unwrap();
        drop(relation);
        let reopened = Relation::new(table_id, 13, path.clone(), table_desc.as_ref())
            .await
            .unwrap();
        assert_version_payloads(&reopened, "must survive reopen").await;
        assert_eq!(
            reopened.visible_value(&key(), &snapshot(7)).await.unwrap(),
            None
        );

        // Rewriting after reopen repopulates the in-memory payload while
        // evicted versions keep reading from the value file.
        reopened
            .write_value(i32_bytes(1), i32_bytes(200), 8)
            .await
            .unwrap();
        assert_eq!(
            reopened.visible_value(&key(), &snapshot(8)).await.unwrap(),
            Some(i32_bytes(200))
        );
        assert_eq!(
            reopened.visible_value(&key(), &snapshot(2)).await.unwrap(),
            Some(i32_bytes(102))
        );

        // Range reads take the same in-memory/file split.
        assert_range_reads_split_in_memory_and_file(&reopened).await;
    })
    .unwrap()
}
