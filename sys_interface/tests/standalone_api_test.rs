#![cfg(all(not(target_arch = "wasm32"), feature = "standalone-adapter"))]

use mudu_contract::database::sql_stmt_text::SQLStmtText;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use mudu_sys::time::system_time_now;
use std::time::UNIX_EPOCH;
use sys_interface::{async_api, host, sync_api};

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
    test_lock().lock().unwrap_or_else(|err| err.into_inner())
}

fn temp_db_path(name: &str) -> PathBuf {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    mudu_sys::env_var::temp_dir().join(format!("sys_interface_{name}_{suffix}.db"))
}

#[test]
fn sync_standalone_kv_and_sql_wrappers_work() {
    let _guard = lock_tests();
    let db_path = temp_db_path("sync");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let session_id = sync_api::mudu_open().unwrap();
    sync_api::mudu_put(session_id, b"k2", b"v2").unwrap();
    sync_api::mudu_put(session_id, b"k1", b"v1").unwrap();

    assert_eq!(
        sync_api::mudu_get(session_id, b"k1").unwrap(),
        Some(b"v1".to_vec())
    );
    assert_eq!(
        sync_api::mudu_range(session_id, b"k1", b"").unwrap(),
        vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
        ]
    );

    let setup = SQLStmtText::new(
        "CREATE TABLE demo(id INT PRIMARY KEY); INSERT INTO demo(id) VALUES (7);".to_string(),
    );
    assert_eq!(sync_api::mudu_batch(session_id, &setup, &()).unwrap(), 1);

    let insert = SQLStmtText::new("INSERT INTO demo(id) VALUES (?1)".to_string());
    assert_eq!(
        sync_api::mudu_command(session_id, &insert, &(9_i32,)).unwrap(),
        1
    );

    let query = SQLStmtText::new("SELECT id FROM demo WHERE id = ?1".to_string());
    let rows = sync_api::mudu_query::<i32>(session_id, &query, &(9_i32,)).unwrap();
    assert_eq!(rows.next_record().unwrap(), Some(9));
    assert_eq!(rows.next_record().unwrap(), None);

    sync_api::mudu_close(session_id).unwrap();
}

#[test]
fn sync_bytes_kv_flow_roundtrips() {
    let _guard = lock_tests();
    let db_path = temp_db_path("sync_bytes");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let open_out = sync_api::mudu_open_bytes(&host::serialize_open_param()).unwrap();
    let session_id = host::deserialize_open_result(&open_out).unwrap();

    let put_in = host::serialize_session_put_param(session_id, b"alpha", b"beta");
    let put_out = sync_api::mudu_put_bytes(&put_in).unwrap();
    host::deserialize_put_result(&put_out).unwrap();

    let get_in = host::serialize_session_get_param(session_id, b"alpha");
    let get_out = sync_api::mudu_get_bytes(&get_in).unwrap();
    assert_eq!(
        host::deserialize_get_result(&get_out).unwrap(),
        Some(b"beta".to_vec())
    );

    let range_in = host::serialize_session_range_param(session_id, b"a", b"z");
    let range_out = sync_api::mudu_range_bytes(&range_in).unwrap();
    assert_eq!(
        host::deserialize_range_result(&range_out).unwrap(),
        vec![(b"alpha".to_vec(), b"beta".to_vec())]
    );

    let close_out = sync_api::mudu_close_bytes(&host::serialize_close_param(session_id)).unwrap();
    host::deserialize_close_result(&close_out).unwrap();
}

#[test]
fn host_invoke_helpers_roundtrip_through_sync_bytes_handlers() {
    let _guard = lock_tests();
    let db_path = temp_db_path("host_helpers");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let session_id = host::invoke_host_open(|input| sync_api::mudu_open_bytes(&input)).unwrap();
    host::invoke_host_session_put(session_id, b"key", b"value", |input| {
        sync_api::mudu_put_bytes(&input)
    })
    .unwrap();
    assert_eq!(
        host::invoke_host_session_get(session_id, b"key", |input| sync_api::mudu_get_bytes(&input))
            .unwrap(),
        Some(b"value".to_vec())
    );
    assert_eq!(
        host::invoke_host_session_range(session_id, b"k", b"z", |input| {
            sync_api::mudu_range_bytes(&input)
        })
        .unwrap(),
        vec![(b"key".to_vec(), b"value".to_vec())]
    );
    host::invoke_host_close(session_id, |input| sync_api::mudu_close_bytes(&input)).unwrap();
}

#[test]
fn async_standalone_kv_and_sql_wrappers_work() {
    let _guard = lock_tests();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db_path = temp_db_path("async");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let session_id = async_api::mudu_open().await.unwrap();
        async_api::mudu_put(session_id, b"k1", b"v1").await.unwrap();
        assert_eq!(
            async_api::mudu_get(session_id, b"k1").await.unwrap(),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            async_api::mudu_range(session_id, b"k1", b"").await.unwrap(),
            vec![(b"k1".to_vec(), b"v1".to_vec())]
        );

        let setup = SQLStmtText::new(
            "CREATE TABLE demo(id INT PRIMARY KEY); INSERT INTO demo(id) VALUES (21);".to_string(),
        );
        assert_eq!(
            async_api::mudu_batch(session_id, &setup, &())
                .await
                .unwrap(),
            1
        );

        let query = SQLStmtText::new("SELECT id FROM demo WHERE id = ?1".to_string());
        let rows = async_api::mudu_query::<i32>(session_id, &query, &(21_i32,))
            .await
            .unwrap();
        assert_eq!(rows.next_record().unwrap(), Some(21));

        async_api::mudu_close(session_id).await.unwrap();
    });
}

#[test]
fn async_bytes_kv_flow_roundtrips() {
    let _guard = lock_tests();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db_path = temp_db_path("async_bytes");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let open_out = async_api::mudu_open_bytes(&host::serialize_open_param())
            .await
            .unwrap();
        let session_id = host::deserialize_open_result(&open_out).unwrap();

        let put_in = host::serialize_session_put_param(session_id, b"left", b"right");
        let put_out = async_api::mudu_put_bytes(&put_in).await.unwrap();
        host::deserialize_put_result(&put_out).unwrap();

        let get_out =
            async_api::mudu_get_bytes(&host::serialize_session_get_param(session_id, b"left"))
                .await
                .unwrap();
        assert_eq!(
            host::deserialize_get_result(&get_out).unwrap(),
            Some(b"right".to_vec())
        );

        let range_out = async_api::mudu_range_bytes(&host::serialize_session_range_param(
            session_id, b"l", b"z",
        ))
        .await
        .unwrap();
        assert_eq!(
            host::deserialize_range_result(&range_out).unwrap(),
            vec![(b"left".to_vec(), b"right".to_vec())]
        );

        let close_out = async_api::mudu_close_bytes(&host::serialize_close_param(session_id))
            .await
            .unwrap();
        host::deserialize_close_result(&close_out).unwrap();
    });
}
