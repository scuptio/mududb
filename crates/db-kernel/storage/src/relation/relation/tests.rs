#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use mudu_sys::env_var::temp_dir;
use mudu_sys::time::instant_now;

use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;

use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::timestamp::Timestamp;
use ::catalog::contract::version_tuple::VersionTuple;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;

use super::*;

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

fn relation_path() -> String {
    temp_dir()
        .join(format!("relation_rebuild_{}", mudu_utils::oid::gen_oid()))
        .to_string_lossy()
        .to_string()
}

fn i32_bytes(v: i32) -> Vec<u8> {
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
        let relation = Arc::new(
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
            readers.push(tokio::spawn(async move {
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
            tokio::spawn(async move {
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

const BENCH_KEYS: i32 = 2000;
const BENCH_BATCH: usize = 13;
const BENCH_ITERS: usize = 300;
const BENCH_WARMUP: usize = 50;
const BENCH_VALUE_LEN: usize = 128;

fn bench_report(name: &str, total_ns: u128, rows: u128) {
    println!(
        "BENCH {name:<34} avg = {:>8} ns ({:>6} ns/row)",
        total_ns,
        total_ns / rows
    );
}

async fn preloaded_relation(table_desc: &TableDesc, table_id: OID, partition_id: OID) -> Relation {
    let relation = Relation::new(table_id, partition_id, relation_path(), table_desc)
        .await
        .unwrap();
    let rows: Vec<(Vec<u8>, Option<Vec<u8>>)> = (0..BENCH_KEYS)
        .map(|i| (i32_bytes(i), Some(vec![7u8; BENCH_VALUE_LEN])))
        .collect();
    relation.write_rows(&rows, 1).await.unwrap();
    relation
}

fn bench_batch_rows(base: i32, xid: u64) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    (0..BENCH_BATCH as i32)
        .map(|j| {
            (
                i32_bytes((base + j) % BENCH_KEYS),
                Some(vec![(xid % 251) as u8; BENCH_VALUE_LEN]),
            )
        })
        .collect()
}

fn bench_key_value_payloads() -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let key_bytes: Vec<Vec<u8>> = (0..BENCH_BATCH as i32).map(i32_bytes).collect();
    let value_bytes: Vec<Vec<u8>> = (0..BENCH_BATCH)
        .map(|j| vec![j as u8; BENCH_VALUE_LEN])
        .collect();
    (key_bytes, value_bytes)
}

// Stage 1: write_rows end to end.
async fn bench_write_rows_total(rel: &Relation) {
    let mut base = 0i32;
    let mut xid = 2u64;
    for _ in 0..BENCH_WARMUP {
        rel.write_rows(&bench_batch_rows(base, xid), xid)
            .await
            .unwrap();
        base += BENCH_BATCH as i32;
        xid += 1;
    }
    let start = instant_now();
    for _ in 0..BENCH_ITERS {
        rel.write_rows(&bench_batch_rows(base, xid), xid)
            .await
            .unwrap();
        base += BENCH_BATCH as i32;
        xid += 1;
    }
    let total_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("write_rows total (13 rows)", total_ns, BENCH_BATCH as u128);
}

// Stage 2: resolve loop only: KeyTuple build + index.get + tuple_id.
async fn bench_resolve_loop(rel: &Relation, rows: &[(Vec<u8>, Option<Vec<u8>>)]) {
    let start = instant_now();
    for _ in 0..BENCH_ITERS {
        let mut resolved = Vec::with_capacity(rows.len());
        for (key, _) in rows {
            let key_tuple = KeyTuple::from(key.clone());
            let row = rel.inner.index.get(&key_tuple).unwrap().unwrap();
            let tuple_id = row.tuple_id().await.unwrap().unwrap();
            resolved.push((key_tuple, row, tuple_id));
        }
        std::hint::black_box(resolved);
    }
    let resolve_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("resolve loop (13 rows)", resolve_ns, BENCH_BATCH as u128);
}

// Stage 3: file insert_batch, batch of 13 and of 1 (fixed vs per-row).
async fn bench_file_insert_batches(rel: &Relation, key_bytes: &[Vec<u8>], value_bytes: &[Vec<u8>]) {
    for i in 0..BENCH_WARMUP as u64 {
        let ts = 100_000 + i;
        let key_rows: Vec<(u64, u64, &[u8])> = key_bytes
            .iter()
            .enumerate()
            .map(|(j, k)| (ts, j as u64 + 1, k.as_slice()))
            .collect();
        rel.inner.key_file.insert_batch(&key_rows).await.unwrap();
        let value_rows: Vec<(u64, u64, &[u8])> = value_bytes
            .iter()
            .enumerate()
            .map(|(j, v)| (ts, j as u64 + 1, v.as_slice()))
            .collect();
        rel.inner
            .value_file
            .insert_batch(&value_rows)
            .await
            .unwrap();
    }
    let start = instant_now();
    for i in 0..BENCH_ITERS as u64 {
        let ts = 200_000 + i;
        let key_rows: Vec<(u64, u64, &[u8])> = key_bytes
            .iter()
            .enumerate()
            .map(|(j, k)| (ts, j as u64 + 1, k.as_slice()))
            .collect();
        rel.inner.key_file.insert_batch(&key_rows).await.unwrap();
    }
    let key13_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("key_file insert_batch(13)", key13_ns, BENCH_BATCH as u128);

    let start = instant_now();
    for i in 0..BENCH_ITERS as u64 {
        let ts = 300_000 + i;
        let value_rows: Vec<(u64, u64, &[u8])> = value_bytes
            .iter()
            .enumerate()
            .map(|(j, v)| (ts, j as u64 + 1, v.as_slice()))
            .collect();
        rel.inner
            .value_file
            .insert_batch(&value_rows)
            .await
            .unwrap();
    }
    let value13_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report(
        "value_file insert_batch(13)",
        value13_ns,
        BENCH_BATCH as u128,
    );

    let start = instant_now();
    for i in 0..BENCH_ITERS as u64 {
        let ts = 400_000 + i;
        rel.inner
            .key_file
            .insert_batch(&[(ts, 1, key_bytes[0].as_slice())])
            .await
            .unwrap();
    }
    let key1_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("key_file insert_batch(1)", key1_ns, 1);

    let start = instant_now();
    for i in 0..BENCH_ITERS as u64 {
        let ts = 500_000 + i;
        rel.inner
            .value_file
            .insert_batch(&[(ts, 1, value_bytes[0].as_slice())])
            .await
            .unwrap();
    }
    let value1_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("value_file insert_batch(1)", value1_ns, 1);
}

// Stage 4: same inserts against a WAL-less raw time-series file,
// isolating the PL WAL append from plan+persist.
async fn bench_raw_insert_batches(value_bytes: &[Vec<u8>]) {
    let raw_path = relation_path();
    let raw_file = TimeSeriesFile::open_ts_file(&raw_path, true).await.unwrap();
    let start = instant_now();
    for i in 0..BENCH_ITERS as u64 {
        let ts = 100_000 + i;
        let value_rows: Vec<(u64, u64, &[u8])> = value_bytes
            .iter()
            .enumerate()
            .map(|(j, v)| (ts, j as u64 + 1, v.as_slice()))
            .collect();
        raw_file.insert_batch(&value_rows).await.unwrap();
    }
    let raw13_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report(
        "raw(no WAL) insert_batch(13)",
        raw13_ns,
        BENCH_BATCH as u128,
    );

    let start = instant_now();
    for i in 0..BENCH_ITERS as u64 {
        let ts = 200_000 + i;
        raw_file
            .insert_batch(&[(ts, 1, value_bytes[0].as_slice())])
            .await
            .unwrap();
    }
    let raw1_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("raw(no WAL) insert_batch(1)", raw1_ns, 1);
}

// Stage 5: version chain + index insert loop only.
async fn bench_version_index_loop(rel: &Relation, rows: &[(Vec<u8>, Option<Vec<u8>>)]) {
    let resolved: Vec<(KeyTuple, DataRow, OID)> = {
        let mut out = Vec::with_capacity(rows.len());
        for (key, _) in rows {
            let key_tuple = KeyTuple::from(key.clone());
            let row = rel.inner.index.get(&key_tuple).unwrap().unwrap();
            let tuple_id = row.tuple_id().await.unwrap().unwrap();
            out.push((key_tuple, row, tuple_id));
        }
        out
    };
    let start = instant_now();
    for i in 0..BENCH_ITERS as u64 {
        let ts = Timestamp::new(600_000 + i, u64::MAX);
        for (key_tuple, row, _) in &resolved {
            row.write(VersionTuple::new(ts.clone(), Vec::new()), None)
                .await
                .unwrap();
            let _ = rel
                .inner
                .index
                .insert(key_tuple.clone(), row.clone())
                .unwrap();
        }
    }
    let version_index_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report(
        "version+index loop (13 rows)",
        version_index_ns,
        BENCH_BATCH as u128,
    );
}

// Stage 6: PL WAL append in isolation: one batch holding the
// record-level delta of a 13-row single-page commit.
async fn bench_wal_isolation(value_bytes: &[Vec<u8>]) {
    use crate::page::PageId;
    use ::wal::pl_batch::{new_pl_batch_writer, PLBatch};
    use ::wal::pl_entry::{PLEntry, PLFileId, PLOp, PLRecord, PageDelta};
    use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogLayout};
    use mudu_utils::oid::gen_oid;

    let wal_dir = temp_dir().join(format!("relation_wal_bench_{}", gen_oid()));
    let backend = ChunkedWorkerLogBackend::new_direct_with_provider(
        WorkerLogLayout::new(wal_dir, gen_oid(), 256 * 1024).unwrap(),
        mudu_sys::default_sys_io_context().provider_arc(),
    )
    .await
    .unwrap();
    let pl_batch = PLBatch::new(vec![PLEntry {
        file: PLFileId {
            partition_id: 1,
            table_id: 2,
            file_index: 0,
        },
        ops: vec![PLOp::PageDelta(PageDelta {
            page_id: PageId::new(0),
            init: None,
            links: None,
            removes: Vec::new(),
            upserts: value_bytes
                .iter()
                .enumerate()
                .map(|(j, v)| PLRecord {
                    timestamp: 700_000,
                    tuple_id: j as u64 + 1,
                    payload: v.clone(),
                })
                .collect(),
        })],
    }]);
    let start = instant_now();
    for _ in 0..BENCH_ITERS {
        std::hint::black_box(backend.serialize_entry(&pl_batch).unwrap());
    }
    let wal_ser_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("wal serialize (13-record batch)", wal_ser_ns, 1);

    let writer = new_pl_batch_writer(backend.clone());
    for _ in 0..BENCH_WARMUP {
        writer.append(&pl_batch).await.unwrap();
    }
    let start = instant_now();
    for _ in 0..BENCH_ITERS {
        writer.append(&pl_batch).await.unwrap();
    }
    let wal_append_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("wal append (13-record batch)", wal_append_ns, 1);
}

// Stage 7: page-level primitives in isolation: one record insert into
// a page image (includes the full-page tailer CRC32 refresh) and
// the raw CRC32 over a page-sized buffer.
fn bench_page_primitives(value_bytes: &[Vec<u8>]) {
    use crate::page::page_block_ref::PAGE_SIZE;
    use crate::page::page_block_ref_mut::PageBlockRefMut;
    use crate::page::PageId;

    let mut page_buf = vec![0u8; PAGE_SIZE];
    PageBlockRefMut::new(&mut page_buf)
        .init_empty(PageId::new(0))
        .unwrap();
    let mut inserts = 0u64;
    let start = instant_now();
    for ts in (1u64..).take(BENCH_ITERS * 4) {
        if PageBlockRefMut::new(&mut page_buf)
            .insert_record(ts, 1, &value_bytes[0])
            .is_err()
        {
            page_buf.fill(0);
            PageBlockRefMut::new(&mut page_buf)
                .init_empty(PageId::new(0))
                .unwrap();
        }
        inserts += 1;
    }
    let page_insert_ns = start.elapsed().as_nanos() / inserts as u128;
    bench_report("page insert_record (128B)", page_insert_ns, 1);

    let start = instant_now();
    for _ in 0..BENCH_ITERS {
        std::hint::black_box(mudu::common::crc::crc32(std::hint::black_box(
            &page_buf[..PAGE_SIZE - 12],
        )));
    }
    let crc32_ns = start.elapsed().as_nanos() / BENCH_ITERS as u128;
    bench_report("crc32(4084B)", crc32_ns, 1);
}

#[test]
#[ignore = "micro-benchmark; run on demand with --ignored --nocapture"]
fn write_path_breakdown() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = test_schema();
        let table_desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();

        let rel_a = preloaded_relation(table_desc.as_ref(), schema.id(), 11).await;
        let rel_b = preloaded_relation(table_desc.as_ref(), schema.id(), 12).await;

        // 1. write_rows end to end.
        bench_write_rows_total(&rel_a).await;

        // 2. resolve loop only: KeyTuple build + index.get + tuple_id.
        // Stage 1 consumed xids 2..=(2 + WARMUP + ITERS); probe rows carry
        // the next xid's payload pattern, as before.
        let rows = bench_batch_rows(0, 2 + (BENCH_WARMUP + BENCH_ITERS) as u64);
        bench_resolve_loop(&rel_b, &rows).await;

        let (key_bytes, value_bytes) = bench_key_value_payloads();

        // 3. file insert_batch, batch of 13 and of 1 (fixed vs per-row).
        bench_file_insert_batches(&rel_b, &key_bytes, &value_bytes).await;

        // 4. Same inserts against a WAL-less raw time-series file,
        // isolating the PL WAL append from plan+persist.
        bench_raw_insert_batches(&value_bytes).await;

        // 5. version chain + index insert loop only.
        bench_version_index_loop(&rel_b, &rows).await;

        // 6. PL WAL append in isolation.
        bench_wal_isolation(&value_bytes).await;

        // 7. Page-level primitives in isolation.
        bench_page_primitives(&value_bytes);
    })
    .unwrap()
}
