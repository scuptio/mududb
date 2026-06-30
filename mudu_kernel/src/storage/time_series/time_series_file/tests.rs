use super::{TimeSeriesFile, TimeSeriesFileIdentity};
use crate::storage::page::page_block_ref::PAGE_SIZE;
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
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

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
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

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
            let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();
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
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

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
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

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
        let mut file = TimeSeriesFile::open_ts_file(&path, true).await.unwrap();

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

    let mut file =
        TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0xfeed_beef, true)
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

        let mut file =
            TimeSeriesFile::open_relation_file(&base, identity.clone(), 0x1234_5678, true)
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

        let mut file = TimeSeriesFile::open_relation_file_with_fs(
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

    let mut file = TimeSeriesFile::open_relation_file_sync(&base, identity.clone(), 0x2, true)
        .await
        .unwrap();
    file.insert(42, 9, b"payload").await.unwrap();
    file.delete_file().await.unwrap();

    let stray = TimeSeriesFile::open_ts_file_sync(&path, true)
        .await
        .unwrap();
    stray.close().await.unwrap();
    assert!(path.exists());

    let err = TimeSeriesFile::open_relation_file_sync(&base, identity, 0x2, false)
        .await
        .err()
        .unwrap();
    assert!(!path.exists());
    assert!(err.to_string().contains("open file error"));
    mudu_sys::fs::sync::remove_dir_all(base).unwrap();
}
