use super::{TimeSeriesFile, TimeSeriesFileIdentity};
use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::storage::page::PageId;
use mudu_sys::common::provider_type::ProviderType;
use mudu_sys::provider::create_io_provider;
use mudu_sys::task::async_::block_on_async_current;
use mudu_utils::log::log_setup;
use project_root::get_project_root;

fn temp_ts_path(name: &str) -> std::path::PathBuf {
    let root = get_project_root().unwrap();
    root.join("target").join("tmp").join(format!(
        "tsf-{}-{}.dat",
        name,
        mudu_sys::random::uuid_v4()
    ))
}

fn temp_relation_base(name: &str) -> std::path::PathBuf {
    let root = get_project_root().unwrap();
    root.join("target").join("tmp").join(format!(
        "tsf-rel-{}-{}",
        name,
        mudu_sys::random::uuid_v4()
    ))
}

fn payload(byte: u8, len: usize) -> Vec<u8> {
    vec![byte; len]
}

#[test]
fn open_create_empty_file() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("empty");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();
        assert_eq!(file.page_count(), PageId::new(0));
        assert_eq!(file.head_page_id(), None);
        assert_eq!(file.tail_page_id(), None);
        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn insert_get_update_delete_roundtrip() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("roundtrip");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        file.insert(100, 1, b"v1").await.unwrap();
        file.insert(90, 2, b"v2").await.unwrap();
        file.insert(100, 1, b"v1-new").await.unwrap();

        let row = file.get(100, 1).await.unwrap().unwrap();
        assert_eq!(row.payload, b"v1-new");
        assert_eq!(row.timestamp, 100);
        assert_eq!(row.tuple_id, 1);

        let row = file.get(90, 2).await.unwrap().unwrap();
        assert_eq!(row.payload, b"v2");

        assert!(file.delete(90, 2).await.unwrap());
        assert_eq!(file.get(90, 2).await.unwrap(), None);
        assert!(!file.delete(90, 2).await.unwrap());

        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn scan_range_returns_sorted_records() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("scan");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        file.insert(120, 4, b"d").await.unwrap();
        file.insert(100, 2, b"b").await.unwrap();
        file.insert(100, 1, b"a").await.unwrap();
        file.insert(110, 3, b"c").await.unwrap();
        file.insert(90, 5, b"e").await.unwrap();

        let rows = file.scan_range(95, 115).await.unwrap();
        let keys: Vec<(u64, u64, Vec<u8>)> = rows
            .into_iter()
            .map(|row| (row.timestamp, row.tuple_id, row.payload))
            .collect();
        assert_eq!(
            keys,
            vec![
                (100, 1, b"a".to_vec()),
                (100, 2, b"b".to_vec()),
                (110, 3, b"c".to_vec()),
            ]
        );

        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn reopen_preserves_records() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("reopen");
        {
            let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();
            file.insert(100, 1, b"alpha").await.unwrap();
            file.insert(80, 2, b"beta").await.unwrap();
            file.flush().await.unwrap();
            file.close().await.unwrap();
        }

        let file = TimeSeriesFile::open_ts_file(&path, false).await.unwrap();
        let row = file.get(100, 1).await.unwrap().unwrap();
        assert_eq!(row.payload, b"alpha");
        let row = file.get(80, 2).await.unwrap().unwrap();
        assert_eq!(row.payload, b"beta");
        assert_eq!(
            file.scan_range(0, 200)
                .await
                .unwrap()
                .into_iter()
                .map(|row| (row.timestamp, row.tuple_id))
                .collect::<Vec<_>>(),
            vec![(80, 2), (100, 1)]
        );
        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn insert_creates_multiple_pages_when_page_is_full() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("split");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        for idx in 0..16u64 {
            let ts = 10_000 - idx;
            let data = payload((idx % 251) as u8, 700);
            file.insert(ts, idx, &data).await.unwrap();
        }

        assert!(file.page_count() > 1);
        assert!(file.head_page_id().is_some());
        assert!(file.tail_page_id().is_some());

        for idx in 0..16u64 {
            let ts = 10_000 - idx;
            let row = file.get(ts, idx).await.unwrap().unwrap();
            assert_eq!(row.timestamp, ts);
            assert_eq!(row.tuple_id, idx);
            assert_eq!(row.payload.len(), 700);
        }

        let rows = file.scan_range(9_980, 10_000).await.unwrap();
        assert_eq!(rows.len(), 16);
        assert!(rows.iter().all(|row| row.payload.len() == 700));

        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn cached_pages_are_reused_after_writes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("cache");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        file.insert(100, 1, b"cached").await.unwrap();
        let page_count = file.page_count();
        assert_eq!(page_count, 1);

        let first = file.get(100, 1).await.unwrap().unwrap();
        let second = file.get(100, 1).await.unwrap().unwrap();
        assert_eq!(first.payload, second.payload);
        assert_eq!(first.page_id, 0);

        let file_len = mudu_sys::fs::sync::metadata(file.path()).unwrap().len() as usize;
        assert_eq!(file_len % PAGE_SIZE, 0);

        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn integrated_api_flow_covers_all_public_operations() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("integrated");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        assert_eq!(file.page_count(), 0);
        assert_eq!(file.head_page_id(), None);
        assert_eq!(file.tail_page_id(), None);
        assert_eq!(file.get(1, 1).await.unwrap(), None);
        assert!(file.scan_range(1, 10).await.unwrap().is_empty());
        assert!(!file.delete(1, 1).await.unwrap());

        for idx in 0..12u64 {
            let ts = 1_000 - idx;
            let value = payload((idx % 251) as u8, 768);
            file.insert(ts, idx, &value).await.unwrap();
        }

        assert!(file.page_count() > 1);
        let head = file.head_page_id().unwrap();
        let tail = file.tail_page_id().unwrap();
        assert!(head <= tail);

        for idx in 0..12u64 {
            let ts = 1_000 - idx;
            let row = file.get(ts, idx).await.unwrap().unwrap();
            assert_eq!(row.timestamp, ts);
            assert_eq!(row.tuple_id, idx);
            assert_eq!(row.payload, payload((idx % 251) as u8, 768));
        }

        let rows = file.scan_range(993, 1_000).await.unwrap();
        let keys: Vec<(u64, u64)> = rows
            .iter()
            .map(|row| (row.timestamp, row.tuple_id))
            .collect();
        assert_eq!(
            keys,
            vec![
                (993, 7),
                (994, 6),
                (995, 5),
                (996, 4),
                (997, 3),
                (998, 2),
                (999, 1),
                (1000, 0),
            ]
        );

        file.insert(997, 3, b"updated").await.unwrap();
        let updated = file.get(997, 3).await.unwrap().unwrap();
        assert_eq!(updated.payload, b"updated");

        assert!(file.delete(995, 5).await.unwrap());
        assert_eq!(file.get(995, 5).await.unwrap(), None);
        assert!(!file.delete(995, 5).await.unwrap());

        file.flush().await.unwrap();
        let persisted_page_count = file.page_count();
        let persisted_head = file.head_page_id();
        let persisted_tail = file.tail_page_id();
        file.close().await.unwrap();

        let reopened = TimeSeriesFile::open_ts_file(&path, false).await.unwrap();
        assert_eq!(reopened.page_count(), persisted_page_count);
        assert_eq!(reopened.head_page_id(), persisted_head);
        assert_eq!(reopened.tail_page_id(), persisted_tail);
        assert_eq!(reopened.get(995, 5).await.unwrap(), None);
        assert_eq!(
            reopened.get(997, 3).await.unwrap().unwrap().payload,
            b"updated"
        );

        let reopened_rows = reopened.scan_range(989, 1_000).await.unwrap();
        let reopened_keys: Vec<(u64, u64)> = reopened_rows
            .iter()
            .map(|row| (row.timestamp, row.tuple_id))
            .collect();
        assert_eq!(
            reopened_keys,
            vec![
                (989, 11),
                (990, 10),
                (991, 9),
                (992, 8),
                (993, 7),
                (994, 6),
                (996, 4),
                (997, 3),
                (998, 2),
                (999, 1),
                (1000, 0),
            ]
        );
        reopened.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn wal_recovers_relation_file_after_data_loss() {
    block_on_async_current(async {
        _wal_recovers_relation_file_after_data_loss().await;
    })
}
async fn _wal_recovers_relation_file_after_data_loss() {
    let base = temp_relation_base("recover");
    let identity = TimeSeriesFileIdentity {
        partition_id: 7,
        table_id: 11,
        file_index: 0,
    };
    let path = TimeSeriesFile::relation_file_path(
        &base,
        identity.partition_id,
        identity.table_id,
        identity.file_index,
    );

    let file = TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0xfeed_beef, true)
        .await
        .unwrap();
    file.insert(100, 1, b"alpha").await.unwrap();
    file.insert(90, 2, b"beta").await.unwrap();
    file.delete(90, 2).await.unwrap();
    file.close().await.unwrap();
    mudu_sys::fs::sync::remove_file(&path).unwrap();

    let reopened = TimeSeriesFile::open_relation_file_sync(&base, identity, 0xfeed_beef, false)
        .await
        .unwrap();
    assert_eq!(
        reopened.get(100, 1).await.unwrap().unwrap().payload,
        b"alpha".to_vec()
    );
    assert_eq!(reopened.get(90, 2).await.unwrap(), None);
    reopened.close().await.unwrap();
    mudu_sys::fs::sync::remove_dir_all(base).unwrap();
}

#[test]
fn wal_recovers_empty_file_from_create_record() {
    block_on_async_current(async move {
        _wal_recovers_empty_file_from_create_record().await;
    })
}
async fn _wal_recovers_empty_file_from_create_record() {
    let base = temp_relation_base("create");
    let identity = TimeSeriesFileIdentity {
        partition_id: 17,
        table_id: 23,
        file_index: 1,
    };
    let path = TimeSeriesFile::relation_file_path(
        &base,
        identity.partition_id,
        identity.table_id,
        identity.file_index,
    );

    let file = TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0x1, true)
        .await
        .unwrap();
    file.close_sync().unwrap();
    mudu_sys::fs::sync::remove_file(&path).unwrap();

    let reopened = TimeSeriesFile::open_relation_file_sync(&base, identity, 0x1, false)
        .await
        .unwrap();
    assert_eq!(reopened.page_count(), 0);
    reopened.close_sync().unwrap();
    mudu_sys::fs::sync::remove_dir_all(base).unwrap();
}

#[test]
fn wal_recovers_relation_file_after_data_loss_async() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let base = temp_relation_base("recover-async");
        let identity = TimeSeriesFileIdentity {
            partition_id: 27,
            table_id: 31,
            file_index: 2,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );

        let file = TimeSeriesFile::open_relation_file(&base, identity.clone(), 0x1234_5678, true)
            .await
            .unwrap();
        file.insert(100, 1, b"alpha").await.unwrap();
        file.insert(90, 2, b"beta").await.unwrap();
        file.delete(90, 2).await.unwrap();
        file.close().await.unwrap();
        mudu_sys::fs::sync::remove_file(&path).unwrap();

        let reopened = TimeSeriesFile::open_relation_file(&base, identity, 0x1234_5678, false)
            .await
            .unwrap();
        assert_eq!(
            reopened.get(100, 1).await.unwrap().unwrap().payload,
            b"alpha".to_vec()
        );
        assert_eq!(reopened.get(90, 2).await.unwrap(), None);
        reopened.close_sync().unwrap();
        mudu_sys::fs::sync::remove_dir_all(base).unwrap();
    })
    .unwrap()
}

#[test]
fn wal_recovers_relation_file_with_injected_async_fs() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let base = temp_relation_base("recover-async-fs");
        let identity = TimeSeriesFileIdentity {
            partition_id: 37,
            table_id: 41,
            file_index: 3,
        };
        let path = TimeSeriesFile::relation_file_path(
            &base,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );
        let provider = create_io_provider(ProviderType::Tokio);

        let file = TimeSeriesFile::open_relation_file_with_fs(
            provider.fs_arc(),
            &base,
            identity.clone(),
            0x55aa_aa55,
            true,
        )
        .await
        .unwrap();
        file.insert(100, 1, b"alpha").await.unwrap();
        file.insert(90, 2, b"beta").await.unwrap();
        file.close().await.unwrap();
        mudu_sys::fs::sync::remove_file(&path).unwrap();

        let reopened = TimeSeriesFile::open_relation_file_with_fs(
            provider.fs_arc(),
            &base,
            identity,
            0x55aa_aa55,
            false,
        )
        .await
        .unwrap();
        assert_eq!(
            reopened.get(100, 1).await.unwrap().unwrap().payload,
            b"alpha".to_vec()
        );
        assert_eq!(
            reopened.get(90, 2).await.unwrap().unwrap().payload,
            b"beta".to_vec()
        );
        reopened.close_sync().unwrap();
        mudu_sys::fs::sync::remove_dir_all(base).unwrap();
    })
    .unwrap()
}

#[test]
fn wal_replays_terminal_delete_before_open() {
    log_setup("info");
    block_on_async_current(async move { _wal_replays_terminal_delete_before_open().await })
}
async fn _wal_replays_terminal_delete_before_open() {
    let base = temp_relation_base("delete");
    let identity = TimeSeriesFileIdentity {
        partition_id: 29,
        table_id: 31,
        file_index: 0,
    };
    let path = TimeSeriesFile::relation_file_path(
        &base,
        identity.partition_id,
        identity.table_id,
        identity.file_index,
    );

    let file = TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0x2, true)
        .await
        .unwrap();
    file.insert(42, 9, b"payload").await.unwrap();
    file.delete_file().await.unwrap();

    let stray = TimeSeriesFile::open_ts_file_sync(&path, true)
        .await
        .unwrap();
    stray.close().await.unwrap();
    // `sync_path_exists` instead of `Path::exists` (deterministic backend).
    assert!(mudu_sys::fs::sync::sync_path_exists(&path));

    let err = TimeSeriesFile::open_relation_file_sync(&base, identity, 0x2, false)
        .await
        .err()
        .unwrap();
    assert!(!mudu_sys::fs::sync::sync_path_exists(&path));
    assert!(err.to_string().contains("open file error"));
    mudu_sys::fs::sync::remove_dir_all(base).unwrap();
}

#[test]
fn newer_versions_pack_into_head_page() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("pack_head");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        // Monotonically increasing timestamps (new row versions): they must
        // pack into the head page instead of creating one page per version.
        const VERSIONS: u64 = 32;
        for ts in 1..=VERSIONS {
            file.insert(ts, 1, b"v").await.unwrap();
        }
        assert!(
            file.page_count() < VERSIONS / 2,
            "expected {} versions to pack into far fewer pages, got {}",
            VERSIONS,
            file.page_count()
        );

        // Every version stays readable at its exact timestamp, and the
        // newest version is found from the head without a full chain walk.
        for ts in 1..=VERSIONS {
            let row = file.get(ts, 1).await.unwrap().unwrap();
            assert_eq!(row.timestamp, ts);
            assert_eq!(row.tuple_id, 1);
        }

        // Reopen: chain invariants are validated at open time.
        file.close().await.unwrap();
        let file = TimeSeriesFile::open_ts_file(&path, false).await.unwrap();
        let row = file.get(VERSIONS, 1).await.unwrap().unwrap();
        assert_eq!(row.payload, b"v");
        let row = file.get(1, 1).await.unwrap().unwrap();
        assert_eq!(row.payload, b"v");
        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn flush_writes_back_dirty_pages() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("dirty-flush");
        let file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

        file.insert(100, 1, b"v1").await.unwrap();
        file.insert(90, 2, b"v2").await.unwrap();
        // Deferred page writes: the images live in the page cache and the
        // pages are marked dirty until flush().
        assert!(file.dirty_page_count() > 0);

        file.flush().await.unwrap();
        assert_eq!(file.dirty_page_count(), 0);
        file.close().await.unwrap();

        // A fresh standalone open has no WAL to replay: the flushed data
        // pages must be on disk.
        let reopened = TimeSeriesFile::open_ts_file(&path, false).await.unwrap();
        assert_eq!(reopened.dirty_page_count(), 0);
        assert_eq!(reopened.get(100, 1).await.unwrap().unwrap().payload, b"v1");
        assert_eq!(reopened.get(90, 2).await.unwrap().unwrap().payload, b"v2");
        reopened.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

#[test]
fn wal_recovers_unflushed_dirty_pages_on_reopen() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let base = temp_relation_base("recover-dirty");
        let identity = TimeSeriesFileIdentity {
            partition_id: 43,
            table_id: 47,
            file_index: 0,
        };

        let file = TimeSeriesFile::open_relation_file(&base, identity.clone(), 0xd177_9abc, true)
            .await
            .unwrap();
        file.insert(100, 1, b"alpha").await.unwrap();
        file.insert(90, 2, b"beta").await.unwrap();
        assert!(file.dirty_page_count() > 0);
        // Dropping without close/flush leaves the dirty pages in memory
        // only: recovery must replay the PL WAL and still serve both rows.
        // The PL frames sit in the group-commit queue until it is driven,
        // so drain it here (production workers drive it continuously).
        file.flush_wal_async().await.unwrap();
        drop(file);

        let reopened = TimeSeriesFile::open_relation_file(&base, identity, 0xd177_9abc, false)
            .await
            .unwrap();
        assert_eq!(
            reopened.get(100, 1).await.unwrap().unwrap().payload,
            b"alpha"
        );
        assert_eq!(reopened.get(90, 2).await.unwrap().unwrap().payload, b"beta");
        reopened.close().await.unwrap();
        mudu_sys::fs::sync::remove_dir_all(base).unwrap();
    })
    .unwrap()
}

#[test]
fn concurrent_inserts_and_reads_observe_consistent_chain() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;

        let path = temp_ts_path("concurrent-rw");
        let file = Arc::new(TimeSeriesFile::open_ts_file(&path, true).await.unwrap());
        // tuple_id -> (timestamp, payload), published only after the insert
        // returned, so readers may probe any record visible here and must
        // always find it in the file.
        let inserted = Arc::new(scc::HashMap::new());
        let high_water = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        const RECORDS: u64 = 3000;
        let writer = {
            let file = file.clone();
            let inserted = inserted.clone();
            let high_water = high_water.clone();
            tokio::spawn(async move {
                for i in 0..RECORDS {
                    // Mostly increasing timestamps with a small jitter, so
                    // some inserts land in older pages and force mid-chain
                    // inserts and page splits instead of only head packs.
                    // ~480-byte payloads fill a page after ~8 records, so
                    // 8000 records produce ~1000 pages and cross the dirty
                    // flush watermark several times.
                    let ts = 1000 + i - (i % 5);
                    let payload = payload((i % 251) as u8, 480);
                    if i.is_multiple_of(2000) {
                        eprintln!("[test] writer at {i}");
                    }
                    file.insert(ts, i, &payload).await.unwrap();
                    let _ = inserted.insert_sync(i, (ts, payload));
                    high_water.store(i + 1, Ordering::Release);
                }
            })
        };

        let mut readers = Vec::new();
        for _ in 0..3 {
            let file = file.clone();
            let inserted = inserted.clone();
            let high_water = high_water.clone();
            let stop = stop.clone();
            readers.push(tokio::spawn(async move {
                let mut scans = 0u64;
                while !stop.load(Ordering::Acquire) {
                    let hi = high_water.load(Ordering::Acquire);
                    if hi == 0 {
                        mudu_sys::tokio::task::yield_now().await;
                        continue;
                    }
                    // Mostly recent records (shallow walks, cheap) plus a
                    // few deep probes so chain walks also reach older pages
                    // while the writer splits and seals pages.
                    let recent_begin = hi.saturating_sub(64);
                    for id in recent_begin..hi {
                        let Some(entry) = inserted.get_sync(&id) else {
                            continue;
                        };
                        let (ts, payload) = entry.get().clone();
                        drop(entry);
                        let row = file.get(ts, id).await.unwrap().unwrap_or_else(|| {
                            panic!("inserted record missing: ts={ts} tuple_id={id}")
                        });
                        assert_eq!(row.payload, payload);
                    }
                    for probe in 1..=2u64 {
                        let id = probe * hi / 3;
                        let Some(entry) = inserted.get_sync(&id) else {
                            continue;
                        };
                        let (ts, payload) = entry.get().clone();
                        drop(entry);
                        let row = file.get(ts, id).await.unwrap().unwrap_or_else(|| {
                            panic!("inserted record missing: ts={ts} tuple_id={id}")
                        });
                        assert_eq!(row.payload, payload);
                    }
                    scans += 1;
                    if scans.is_multiple_of(32) {
                        let rows = file.scan_range(0, u64::MAX).await.unwrap();
                        // The writer may have completed (but not yet
                        // published) further inserts, so the scan can only
                        // be ahead of the watermark, never behind it.
                        assert!(rows.len() >= hi as usize);
                    }
                    // Cache hits never pend, so yield explicitly to let the
                    // writer and the other readers make progress.
                    mudu_sys::tokio::task::yield_now().await;
                }
            }));
        }
        writer.await.unwrap();
        stop.store(true, Ordering::Release);
        for reader in readers {
            reader.await.unwrap();
        }

        // Final consistency pass: a full scan must return exactly the
        // inserted set in (timestamp, tuple_id) order.
        let rows = file.scan_range(0, u64::MAX).await.unwrap();
        assert_eq!(rows.len(), RECORDS as usize);
        let mut expected: Vec<(u64, u64, Vec<u8>)> = (0..RECORDS)
            .map(|id| {
                let (ts, payload) = inserted.get_sync(&id).unwrap().get().clone();
                (ts, id, payload)
            })
            .collect();
        expected.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        for (row, (ts, id, payload)) in rows.iter().zip(expected.iter()) {
            assert_eq!(row.timestamp, *ts);
            assert_eq!(row.tuple_id, *id);
            assert_eq!(&row.payload, payload);
        }
        let Ok(file) = Arc::try_unwrap(file) else {
            panic!("readers must have released the file");
        };
        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}

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
