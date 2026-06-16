use mudu_adapter::{backend, config, kv, sqlite};
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use std::env;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use mudu_sys::time::system_time_now;
use std::time::UNIX_EPOCH;

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn temp_db_path(name: &str) -> PathBuf {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    mudu_sys::env_var::temp_dir().join(format!("mudu_adapter_{name}_{suffix}.db"))
}

fn with_connection_env<T>(value: &str, f: impl FnOnce() -> T) -> T {
    let prev = env::var("MUDU_CONNECTION").ok();
    // SAFETY: tests serialize access through test_lock(), so process env mutation is not concurrent here.
    unsafe { env::set_var("MUDU_CONNECTION", value) };
    let result = f();
    match prev {
        Some(prev) => {
            // SAFETY: guarded by the same test mutex.
            unsafe { env::set_var("MUDU_CONNECTION", prev) };
        }
        None => {
            // SAFETY: guarded by the same test mutex.
            unsafe { env::remove_var("MUDU_CONNECTION") };
        }
    }
    result
}

#[test]
fn connection_parses_supported_driver_variants() {
    let _guard = test_lock().lock().unwrap();
    config::reset_db_path_override_for_test();

    with_connection_env("postgres://user:pw@localhost/db", || {
        assert_eq!(config::driver(), config::Driver::Postgres);
        assert_eq!(
            config::postgres_url().as_deref(),
            Some("postgres://user:pw@localhost/db")
        );
    });

    with_connection_env("mysql://user:pw@localhost/db", || {
        assert_eq!(config::driver(), config::Driver::MySql);
        assert_eq!(
            config::mysql_url().as_deref(),
            Some("mysql://user:pw@localhost/db")
        );
    });

    with_connection_env(
        "mudud://127.0.0.1:9527/demo?http_addr=127.0.0.1:8301&async=true",
        || {
            assert_eq!(config::driver(), config::Driver::Mudud);
            assert_eq!(config::mudud_addr().as_deref(), Some("127.0.0.1:9527"));
            assert_eq!(config::mudud_http_addr().as_deref(), Some("127.0.0.1:8301"));
            assert_eq!(config::mudud_app_name().as_deref(), Some("demo"));
            assert!(config::mudud_async_session_loop());
        },
    );

    with_connection_env("sqlite://./adapter_test.db", || {
        assert_eq!(config::driver(), config::Driver::Sqlite);
        assert!(
            config::db_path()
                .to_string_lossy()
                .ends_with("adapter_test.db")
        );
    });
}

#[test]
fn replace_placeholders_formats_supported_sqlite_values() {
    let _guard = test_lock().lock().unwrap();
    let sql = "INSERT INTO demo VALUES (?, ?, ?, ?)";
    let params = (7_i32, 9_i64, 1.5_f32, String::from("abc"));
    let rendered = backend::replace_placeholders(sql, &params).unwrap();
    assert_eq!(rendered, "INSERT INTO demo VALUES (7, 9, 1.5, \"abc\")");
}

#[test]
fn sqlite_session_kv_and_batch_flow_work_end_to_end() {
    let _guard = test_lock().lock().unwrap();
    config::reset_db_path_override_for_test();
    let db_path = temp_db_path("sqlite_kv");
    config::set_db_path(&db_path);

    let session_id = sqlite::mudu_open().unwrap();
    kv::put(session_id, b"k2", b"v2").unwrap();
    kv::put(session_id, b"k1", b"v1").unwrap();
    kv::put(session_id, b"k3", b"v3").unwrap();

    assert_eq!(kv::get(session_id, b"k2").unwrap(), Some(b"v2".to_vec()));
    assert_eq!(
        kv::range(session_id, b"k1", b"k3").unwrap(),
        vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
        ]
    );
    assert_eq!(
        kv::range(session_id, b"k2", b"").unwrap(),
        vec![
            (b"k2".to_vec(), b"v2".to_vec()),
            (b"k3".to_vec(), b"v3".to_vec()),
        ]
    );

    let create = SQLStmtText::new(
        "CREATE TABLE t(id INT PRIMARY KEY, v TEXT); INSERT INTO t(id, v) VALUES (1, 'a');"
            .to_string(),
    );
    assert_eq!(sqlite::mudu_batch(session_id, &create, &()).unwrap(), 1);

    let conn = sqlite::open_connection().unwrap();
    let selected: String = conn
        .query_row("SELECT v FROM t WHERE id = 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(selected, "a");

    sqlite::mudu_close(session_id).unwrap();
    assert!(kv::ensure_session_exists(session_id).is_err());
}

#[test]
fn backend_batch_attempts_mudud_driver_request_instead_of_not_implemented() {
    let _guard = test_lock().lock().unwrap();
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9527/default", || {
        let stmt = SQLStmtText::new("SELECT 1".to_string());
        let err = backend::mudu_batch(1, &stmt, &()).unwrap_err();
        let message = err.to_string();
        assert!(!message.contains("batch syscall is not implemented for mudud adapter"));
    });
}

#[test]
fn sqlite_async_session_kv_query_command_and_batch_work() {
    let _guard = test_lock().lock().unwrap();
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        config::reset_db_path_override_for_test();
        let db_path = temp_db_path("sqlite_async");
        config::set_db_path(&db_path);

        let session_id = backend::mudu_open_async(0).await.unwrap();
        backend::mudu_put_async(session_id, b"k2", b"v2").await.unwrap();
        backend::mudu_put_async(session_id, b"k1", b"v1").await.unwrap();

        assert_eq!(
            backend::mudu_get_async(session_id, b"k1").await.unwrap(),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            backend::mudu_range_async(session_id, b"k1", b"").await.unwrap(),
            vec![
                (b"k1".to_vec(), b"v1".to_vec()),
                (b"k2".to_vec(), b"v2".to_vec()),
            ]
        );

        let setup = SQLStmtText::new(
            "CREATE TABLE demo(id INT PRIMARY KEY, v TEXT); INSERT INTO demo(id, v) VALUES (1, 'a');"
                .to_string(),
        );
        assert_eq!(backend::mudu_batch_async(session_id, &setup, &()).await.unwrap(), 1);

        let insert = SQLStmtText::new("INSERT INTO demo(id, v) VALUES (?1, ?2)".to_string());
        assert_eq!(
            backend::mudu_command_async(session_id, &insert, &(2_i32, String::from("b")))
                .await
                .unwrap(),
            1
        );

        let query = SQLStmtText::new("SELECT v FROM demo WHERE id = ?1".to_string());
        let rows = backend::mudu_query_async::<String>(session_id, &query, &(2_i32,))
            .await
            .unwrap();
        assert_eq!(rows.next_record().unwrap(), Some("b".to_string()));
        assert_eq!(rows.next_record().unwrap(), None);

        backend::mudu_close_async(session_id).await.unwrap();
        assert!(kv::ensure_session_exists(session_id).is_err());
    }).unwrap();
}
