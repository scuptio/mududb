use super::{payload, temp_relation_base, temp_ts_path};
use crate::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::time_series::time_series_file::{TimeSeriesFile, TimeSeriesFileIdentity};

#[test]
fn insert_batch_equivalent_to_single_row_inserts() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path_single = temp_ts_path("batch-eq-single");
        let path_batch = temp_ts_path("batch-eq-batch");
        let single = TimeSeriesFile::open_ts_file(&path_single, true)
            .await
            .unwrap();
        let batch = TimeSeriesFile::open_ts_file(&path_batch, true)
            .await
            .unwrap();

        // Build the same logical mutation sequence for both files: same-ts
        // rows packing and sealing the head page, a newer-ts batch hitting
        // HeadLatest, an in-place update, and an out-of-order row landing
        // mid-chain. ~480-byte payloads cross page boundaries within one
        // batch.
        let payloads: Vec<Vec<u8>> = (0..40u8).map(|i| payload(i, 480)).collect();
        let batches: Vec<Vec<(u64, u64, &[u8])>> = vec![
            (0..20)
                .map(|i| (100u64, i as u64, payloads[i].as_slice()))
                .collect(),
            (0..10)
                .map(|i| (200u64, i as u64, payloads[20 + i].as_slice()))
                .collect(),
            vec![
                (100, 5, payloads[30].as_slice()),
                (50, 31, payloads[31].as_slice()),
                (150, 32, payloads[32].as_slice()),
            ],
        ];

        for rows in &batches {
            for &(ts, tuple_id, bytes) in rows.iter() {
                single.insert(ts, tuple_id, bytes).await.unwrap();
            }
            batch.insert_batch(rows).await.unwrap();
        }

        assert_eq!(single.page_count(), batch.page_count());
        let single_rows = single.scan_range(0, u64::MAX).await.unwrap();
        let batch_rows = batch.scan_range(0, u64::MAX).await.unwrap();
        assert_eq!(
            single_rows
                .iter()
                .map(|row| (row.timestamp, row.tuple_id, row.payload.clone()))
                .collect::<Vec<_>>(),
            batch_rows
                .iter()
                .map(|row| (row.timestamp, row.tuple_id, row.payload.clone()))
                .collect::<Vec<_>>()
        );
        // The in-place update must be visible through the batch path too.
        assert_eq!(
            batch.get(100, 5).await.unwrap().unwrap().payload,
            payloads[30]
        );

        single.close().await.unwrap();
        batch.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path_single);
        let _ = mudu_sys::fs::sync::remove_file(path_batch);
    })
    .unwrap()
}

#[test]
fn insert_batch_wal_recovers_after_data_loss() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let base = temp_relation_base("recover-batch");
        let identity = TimeSeriesFileIdentity {
            partition_id: 53,
            table_id: 59,
            file_index: 0,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );

        let payloads: Vec<Vec<u8>> = (0..20u8).map(|i| payload(i, 480)).collect();
        let file = TimeSeriesFile::open_relation_file(&base, identity.clone(), 0x000b_a7c4, true)
            .await
            .unwrap();
        // One multi-page batch (one PL WAL append) plus a newer-ts batch.
        let first: Vec<(u64, u64, &[u8])> = (0..14)
            .map(|i| (100u64, i as u64, payloads[i].as_slice()))
            .collect();
        file.insert_batch(&first).await.unwrap();
        let second: Vec<(u64, u64, &[u8])> = (0..6)
            .map(|i| (200u64, (14 + i) as u64, payloads[14 + i].as_slice()))
            .collect();
        file.insert_batch(&second).await.unwrap();
        file.close().await.unwrap();
        mudu_sys::fs::sync::remove_file(&path).unwrap();

        let reopened = TimeSeriesFile::open_relation_file(&base, identity, 0x000b_a7c4, false)
            .await
            .unwrap();
        for (i, expected) in payloads.iter().enumerate().take(14) {
            assert_eq!(
                reopened.get(100, i as u64).await.unwrap().unwrap().payload,
                *expected
            );
        }
        for (offset, expected) in payloads.iter().skip(14).enumerate() {
            assert_eq!(
                reopened
                    .get(200, (14 + offset) as u64)
                    .await
                    .unwrap()
                    .unwrap()
                    .payload,
                *expected
            );
        }
        reopened.close().await.unwrap();
        mudu_sys::fs::sync::remove_dir_all(base).unwrap();
    })
    .unwrap()
}

type ExpectedRows = std::collections::BTreeMap<(u64, u64), Vec<u8>>;

async fn insert_rows(
    file: &TimeSeriesFile,
    rows: &[(u64, u64, Vec<u8>)],
    expected: &mut ExpectedRows,
) {
    let refs: Vec<(u64, u64, &[u8])> = rows
        .iter()
        .map(|(ts, tuple_id, bytes)| (*ts, *tuple_id, bytes.as_slice()))
        .collect();
    file.insert_batch(&refs).await.unwrap();
    for (ts, tuple_id, bytes) in rows {
        expected.insert((*ts, *tuple_id), bytes.clone());
    }
}

async fn delete_row(
    file: &TimeSeriesFile,
    timestamp: u64,
    tuple_id: u64,
    expected: &mut ExpectedRows,
) {
    assert!(file.delete(timestamp, tuple_id).await.unwrap());
    assert!(expected.remove(&(timestamp, tuple_id)).is_some());
}

async fn verify_file_contents(file: &TimeSeriesFile, expected: &ExpectedRows) {
    let rows = file.scan_range(0, u64::MAX).await.unwrap();
    let got: Vec<(u64, u64, Vec<u8>)> = rows
        .into_iter()
        .map(|row| (row.timestamp, row.tuple_id, row.payload))
        .collect();
    let want: Vec<(u64, u64, Vec<u8>)> = expected
        .iter()
        .map(|((ts, tuple_id), bytes)| (*ts, *tuple_id, bytes.clone()))
        .collect();
    if got != want {
        let got_keys: Vec<(u64, u64)> = got.iter().map(|(ts, tid, _)| (*ts, *tid)).collect();
        let want_keys: Vec<(u64, u64)> = want.iter().map(|(ts, tid, _)| (*ts, *tid)).collect();
        eprintln!("got keys:  {got_keys:?}");
        eprintln!("want keys: {want_keys:?}");
        for (g, w) in got.iter().zip(want.iter()) {
            if g != w {
                eprintln!(
                    "first diff: got ({}, {}) len {} want ({}, {}) len {}",
                    g.0,
                    g.1,
                    g.2.len(),
                    w.0,
                    w.1,
                    w.2.len()
                );
                break;
            }
        }
    }
    assert_eq!(got, want);
    for ((ts, tuple_id), bytes) in expected {
        let row = file.get(*ts, *tuple_id).await.unwrap().unwrap();
        assert_eq!(&row.payload, bytes);
    }
}

/// Builds one mixed mutation sequence covering every page-level WAL delta
/// shape: a same-ts multi-row batch that splits pages inside one commit,
/// newer-ts rows packing and sealing the head page, an in-place update of a
/// unique-ts key, mid-chain `Before` inserts, an oldest-ts tail append, and
/// record deletes.
///
/// `key_base` offsets every tuple id so a second round after recovery
/// inserts only fresh keys. The in-place update targets a unique-ts key on
/// purpose: when one timestamp spans several pages, re-inserting a key that
/// lives on a later same-ts page appends a duplicate instead of updating
/// (the insert location search only probes the first page whose bounds
/// contain the timestamp), so this model keeps one entry per key.
async fn write_mixed_batches(
    file: &TimeSeriesFile,
    expected: &mut ExpectedRows,
    payload_tag: u8,
    key_base: u64,
) {
    // Same-ts rows exceeding one page: splits within a single batch commit.
    let rows: Vec<(u64, u64, Vec<u8>)> = (0..20u64)
        .map(|i| {
            (
                100,
                key_base + i,
                payload(payload_tag.wrapping_add(i as u8), 480),
            )
        })
        .collect();
    insert_rows(file, &rows, expected).await;
    // Newer-ts rows: pack into the head page, then seal it and start a
    // fresh head page when full.
    let rows: Vec<(u64, u64, Vec<u8>)> = (0..10u64)
        .map(|i| (200, key_base + 100 + i, payload(payload_tag ^ 0x5a, 480)))
        .collect();
    insert_rows(file, &rows, expected).await;
    // An oldest-ts row appended after the tail plus two unique-ts rows
    // landing mid-chain between the ts=200 and ts=100 pages.
    let rows: Vec<(u64, u64, Vec<u8>)> = vec![
        (50, key_base + 300, payload(payload_tag ^ 0x11, 480)),
        (150, key_base + 301, payload(payload_tag ^ 0x22, 480)),
        (175, key_base + 302, payload(payload_tag ^ 0x33, 480)),
    ];
    insert_rows(file, &rows, expected).await;
    // In-place payload replace (different size) on the unique-ts key.
    let rows: Vec<(u64, u64, Vec<u8>)> =
        vec![(150, key_base + 301, payload(payload_tag ^ 0xa5, 300))];
    insert_rows(file, &rows, expected).await;
    delete_row(file, 100, key_base + 7, expected).await;
    delete_row(file, 200, key_base + 103, expected).await;
    assert!(!file.delete(100, key_base + 7).await.unwrap());
}

#[test]
fn wal_recovers_unflushed_mixed_batches_and_continued_writes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let base = temp_relation_base("recover-mixed");
        let identity = TimeSeriesFileIdentity {
            partition_id: 61,
            table_id: 67,
            file_index: 0,
        };
        let mut expected = ExpectedRows::new();

        let file = TimeSeriesFile::open_relation_file(&base, identity.clone(), 0xd1a7_0001, true)
            .await
            .unwrap();
        write_mixed_batches(&file, &mut expected, 0x30, 0).await;
        assert!(file.page_count() > 1);
        assert!(file.dirty_page_count() > 0);
        let page_count = file.page_count();
        let head_page_id = file.head_page_id();
        let tail_page_id = file.tail_page_id();
        // Crash: no flush, no close. Recovery must rebuild every unflushed
        // page from the record-level WAL deltas. The PL frames sit in the
        // group-commit queue until it is driven, so drain it first
        // (production workers drive it continuously).
        file.flush_wal_async().await.unwrap();
        drop(file);

        let file = TimeSeriesFile::open_relation_file(&base, identity.clone(), 0xd1a7_0001, false)
            .await
            .unwrap();
        assert_eq!(file.page_count(), page_count);
        assert_eq!(file.head_page_id(), head_page_id);
        assert_eq!(file.tail_page_id(), tail_page_id);
        verify_file_contents(&file, &expected).await;

        // Continue writing after recovery: the same mixed sequence again
        // (splits now hit pages rebuilt by recovery), then crash and
        // recover once more.
        write_mixed_batches(&file, &mut expected, 0x70, 1000).await;
        let page_count = file.page_count();
        let head_page_id = file.head_page_id();
        let tail_page_id = file.tail_page_id();
        file.flush_wal_async().await.unwrap();
        drop(file);

        let file = TimeSeriesFile::open_relation_file(&base, identity, 0xd1a7_0001, false)
            .await
            .unwrap();
        assert_eq!(file.page_count(), page_count);
        assert_eq!(file.head_page_id(), head_page_id);
        assert_eq!(file.tail_page_id(), tail_page_id);
        verify_file_contents(&file, &expected).await;
        file.close().await.unwrap();
        mudu_sys::fs::sync::remove_dir_all(base).unwrap();
    })
    .unwrap()
}

#[test]
fn wal_replay_after_full_flush_leaves_data_file_byte_stable() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let base = temp_relation_base("recover-stable");
        let identity = TimeSeriesFileIdentity {
            partition_id: 71,
            table_id: 73,
            file_index: 0,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );
        let mut expected = ExpectedRows::new();

        let file = TimeSeriesFile::open_relation_file(&base, identity.clone(), 0xd1a7_0002, true)
            .await
            .unwrap();
        write_mixed_batches(&file, &mut expected, 0x40, 0).await;
        verify_file_contents(&file, &expected).await;
        file.flush().await.unwrap();
        assert_eq!(file.dirty_page_count(), 0);
        let bytes_before = mudu_sys::fs::sync::read(&path).unwrap();
        assert!(!bytes_before.is_empty());
        drop(file);

        // Every WAL delta is already reflected in the flushed data file, so
        // recovery must replay as pure no-ops: no page is rewritten and the
        // file stays byte-identical.
        let file = TimeSeriesFile::open_relation_file(&base, identity, 0xd1a7_0002, false)
            .await
            .unwrap();
        verify_file_contents(&file, &expected).await;
        file.close().await.unwrap();
        let bytes_after = mudu_sys::fs::sync::read(&path).unwrap();
        assert_eq!(bytes_before, bytes_after);
        mudu_sys::fs::sync::remove_dir_all(base).unwrap();
    })
    .unwrap()
}

#[test]
fn persisted_pages_carry_valid_checksums_after_flush() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("crc-persist");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        // Enough rows to span several pages, including a split.
        for idx in 0..32u64 {
            let ts = 10_000 - idx;
            let data = payload((idx % 251) as u8, 500);
            file.insert(ts, idx, &data).await.unwrap();
        }
        assert!(file.page_count() > 1);
        file.flush().await.unwrap();
        file.close().await.unwrap();

        // The deferred tailer checksum must be finalized by the persistence
        // points: every page on disk passes full layout validation.
        let bytes = mudu_sys::fs::sync::read(&path).unwrap();
        assert_eq!(bytes.len() % PAGE_SIZE, 0);
        assert!(!bytes.is_empty());
        for chunk in bytes.chunks(PAGE_SIZE) {
            PageBlockRef::try_new(chunk)
                .unwrap()
                .validate_layout()
                .unwrap();
        }
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn open_rejects_page_with_corrupted_checksum_on_disk() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("crc-corrupt");
        {
            let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();
            file.insert(100, 1, b"v1").await.unwrap();
            file.flush().await.unwrap();
            file.close().await.unwrap();
        }

        // Flip a byte in the page's free region: the page checksum covers
        // the whole page except the tailer, so the open-time disk validation
        // must still detect the corruption.
        let mut bytes = mudu_sys::fs::sync::read(&path).unwrap();
        bytes[PAGE_SIZE / 2] ^= 0xFF;
        mudu_sys::fs::sync::write(&path, &bytes).unwrap();

        let err = match TimeSeriesFile::open_ts_file(&path, false).await {
            Ok(_) => panic!("corrupted page on disk must fail open"),
            Err(err) => err,
        };
        let msg = format!("{err:?}");
        assert!(msg.contains("checksum mismatch"), "unexpected error: {msg}");
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}
