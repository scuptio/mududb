use super::{payload, temp_relation_base, temp_ts_path};
use crate::time_series::time_series_file::{TimeSeriesFile, TimeSeriesFileIdentity};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

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

const CONCURRENT_RECORDS: u64 = 3000;

// tuple_id -> (timestamp, payload), published only after the insert
// returned, so readers may probe any record visible here and must
// always find it in the file.
type InsertedMap = scc::HashMap<u64, (u64, Vec<u8>)>;

async fn assert_record_present(file: &TimeSeriesFile, inserted: &InsertedMap, id: u64) {
    let Some(entry) = inserted.get_sync(&id) else {
        return;
    };
    let (ts, payload) = entry.get().clone();
    drop(entry);
    let row = file
        .get(ts, id)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("inserted record missing: ts={ts} tuple_id={id}"));
    assert_eq!(row.payload, payload);
}

fn spawn_writer(
    file: Arc<TimeSeriesFile>,
    inserted: Arc<InsertedMap>,
    high_water: Arc<AtomicU64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        for i in 0..CONCURRENT_RECORDS {
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
}

fn spawn_reader(
    file: Arc<TimeSeriesFile>,
    inserted: Arc<InsertedMap>,
    high_water: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
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
                assert_record_present(&file, &inserted, id).await;
            }
            for probe in 1..=2u64 {
                assert_record_present(&file, &inserted, probe * hi / 3).await;
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
    })
}

// Final consistency pass: a full scan must return exactly the
// inserted set in (timestamp, tuple_id) order.
async fn assert_final_scan(file: &TimeSeriesFile, inserted: &InsertedMap) {
    let rows = file.scan_range(0, u64::MAX).await.unwrap();
    assert_eq!(rows.len(), CONCURRENT_RECORDS as usize);
    let mut expected: Vec<(u64, u64, Vec<u8>)> = (0..CONCURRENT_RECORDS)
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
}

#[test]
fn concurrent_inserts_and_reads_observe_consistent_chain() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let path = temp_ts_path("concurrent-rw");
        let file = Arc::new(TimeSeriesFile::open_ts_file(&path, true).await.unwrap());
        let inserted = Arc::new(InsertedMap::new());
        let high_water = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let writer = spawn_writer(file.clone(), inserted.clone(), high_water.clone());

        let mut readers = Vec::new();
        for _ in 0..3 {
            readers.push(spawn_reader(
                file.clone(),
                inserted.clone(),
                high_water.clone(),
                stop.clone(),
            ));
        }
        writer.await.unwrap();
        stop.store(true, Ordering::Release);
        for reader in readers {
            reader.await.unwrap();
        }

        assert_final_scan(&file, &inserted).await;
        let Ok(file) = Arc::try_unwrap(file) else {
            panic!("readers must have released the file");
        };
        file.close().await.unwrap();
        let _ = mudu_sys::fs::sync::remove_file(path);
    })
    .unwrap()
}
