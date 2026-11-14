//! In-process coverage for the SQLite backend: session lifecycle, KV, SQL,
//! and relation operations against a temporary database file.
//!
//! `MUDU_CONNECTION` is process-global, so every test here runs under
//! `config::test_guard()` with its own overridden db path.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{config, sqlite};
use mudu_contract::database::sql_stmt_text::SQLStmtText;

fn with_temp_db(test: impl FnOnce()) {
    let _guard = config::test_guard();
    config::set_db_path(mudu_sys::env_var::temp_dir().join(format!(
        "mudu-adapter-sqlite-test-{}",
        mudu_sys::random::uuid_v4()
    )));
    test();
    config::reset_db_path_override_for_test();
}

fn stmt(sql: &str) -> SQLStmtText {
    SQLStmtText::new(sql.to_string())
}

#[test]
fn sqlite_session_kv_sql_and_relation_roundtrip() {
    with_temp_db(|| {
        // Session + KV surface.
        let session = sqlite::mudu_open().unwrap();
        sqlite::mudu_put(session, b"k1", b"v1").unwrap();
        assert_eq!(
            sqlite::mudu_get(session, b"k1").unwrap(),
            Some(b"v1".to_vec())
        );
        assert_eq!(sqlite::mudu_get(session, b"missing").unwrap(), None);
        let items = sqlite::mudu_range(session, b"", b"z").unwrap();
        assert!(items.iter().any(|(k, v)| k == b"k1" && v == b"v1"));

        // SQL surface on the same session.
        sqlite::mudu_command(
            session,
            &stmt("CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER)"),
            &(),
        )
        .unwrap();
        sqlite::mudu_command(session, &stmt("INSERT INTO t VALUES (1, 42)"), &()).unwrap();
        sqlite::mudu_batch(session, &stmt("INSERT INTO t VALUES (2, 43)"), &()).unwrap();
        let result =
            sqlite::mudu_query::<i32>(session, &stmt("SELECT v FROM t WHERE id = 1"), &()).unwrap();
        assert_eq!(result.next_record().unwrap(), Some(42));

        // Relation read-modify-write on the same table (i64 big-endian datums).
        let key = vec![(0u64, 1i32.to_be_bytes().to_vec())];
        let row = sqlite::mudu_relation_get(session, "t", &key, &[1]).unwrap();
        assert_eq!(row, Some(vec![Some(42i32.to_be_bytes().to_vec())]));

        sqlite::mudu_relation_insert(
            session,
            "t",
            &[(0u64, 7i32.to_be_bytes().to_vec())],
            &[(1u64, 77i32.to_be_bytes().to_vec())],
        )
        .unwrap();
        let row =
            sqlite::mudu_relation_get(session, "t", &[(0u64, 7i32.to_be_bytes().to_vec())], &[1])
                .unwrap();
        assert_eq!(row, Some(vec![Some(77i32.to_be_bytes().to_vec())]));

        let updated = sqlite::mudu_relation_update(
            session,
            "t",
            &key,
            &[(1u64, 100i32.to_be_bytes().to_vec())],
            &[],
        )
        .unwrap();
        assert_eq!(updated, 1);
        let row = sqlite::mudu_relation_get(session, "t", &key, &[1]).unwrap();
        assert_eq!(row, Some(vec![Some(100i32.to_be_bytes().to_vec())]));

        sqlite::mudu_close(session).unwrap();
        // Closing an unknown session fails.
        assert!(sqlite::mudu_close(session).is_err());
    });
}

#[test]
fn sqlite_async_apis_roundtrip() {
    with_temp_db(|| {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let session = sqlite::mudu_open_async().await.unwrap();
            sqlite::mudu_put_async(session, b"k", b"v").await.unwrap();
            assert_eq!(
                sqlite::mudu_get_async(session, b"k").await.unwrap(),
                Some(b"v".to_vec())
            );
            let items = sqlite::mudu_range_async(session, b"", b"z").await.unwrap();
            assert!(!items.is_empty());

            sqlite::mudu_command_async(
                session,
                &stmt("CREATE TABLE t2 (id INTEGER PRIMARY KEY, v INTEGER)"),
                &(),
            )
            .await
            .unwrap();
            sqlite::mudu_batch_async(session, &stmt("INSERT INTO t2 VALUES (1, 5)"), &())
                .await
                .unwrap();
            let result = sqlite::mudu_query_async::<i32>(session, &stmt("SELECT v FROM t2"), &())
                .await
                .unwrap();
            assert_eq!(result.next_record().unwrap(), Some(5));

            sqlite::mudu_relation_insert_async(
                session,
                "t2",
                &[(0u64, 9i32.to_be_bytes().to_vec())],
                &[(1u64, 90i32.to_be_bytes().to_vec())],
            )
            .await
            .unwrap();
            let updated = sqlite::mudu_relation_update_async(
                session,
                "t2",
                &[(0u64, 9i32.to_be_bytes().to_vec())],
                &[(1u64, 91i32.to_be_bytes().to_vec())],
                &[],
            )
            .await
            .unwrap();
            assert_eq!(updated, 1);
            let row = sqlite::mudu_relation_get_async(
                session,
                "t2",
                &[(0u64, 9i32.to_be_bytes().to_vec())],
                &[1],
            )
            .await
            .unwrap();
            assert_eq!(row, Some(vec![Some(91i32.to_be_bytes().to_vec())]));

            sqlite::mudu_close_async(session).await.unwrap();
        })
        .unwrap();
    });
}
