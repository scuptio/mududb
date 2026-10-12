use super::temp_relation_base;
use crate::time_series::time_series_file::{TimeSeriesFile, TimeSeriesFileIdentity};
use mudu_sys::common::provider_type::ProviderType;
use mudu_sys::provider::create_io_provider;
use mudu_sys::task::async_::block_on_async_current;
use mudu_utils::log::log_setup;

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
