//! The `write_path_breakdown` micro-benchmark and its helpers: `#[ignore]`d,
//! run on demand with `--ignored --nocapture`. Split out of `tests.rs` so the
//! unit-test file stays fully exercised by default test runs; this file is
//! excluded from coverage reports like `server/src/stmt_path_bench.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use mudu_sys::env_var::temp_dir;
use mudu_sys::time::instant_now;

use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::timestamp::Timestamp;
use ::catalog::contract::version_tuple::VersionTuple;

use super::tests::{i32_bytes, relation_path, test_schema};
use super::*;
use catalog::contract::data_row::DataRow;

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
            let row = rel.inner.index.get(&key_tuple).await.unwrap().unwrap();
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
            let row = rel.inner.index.get(&key_tuple).await.unwrap().unwrap();
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
                .await
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
