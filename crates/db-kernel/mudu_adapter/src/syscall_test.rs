//! End-to-end coverage of the syscall facade with the SQLite backend plus the
//! local-fs emulation: session lifecycle, KV, SQL, relation, and fs syscalls.
//!
//! `MUDU_CONNECTION` is process-global, so every test runs under
//! `config::test_guard()` with its own overridden db path.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{config, syscall};
use mudu::common::id::OID;
use mudu_binding::universal::uni_relation::UniRelationDelta;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::sql_stmt_text::SQLStmtText;

fn with_temp_backend(test: impl FnOnce()) {
    let _guard = config::test_guard();
    config::set_db_path(mudu_sys::env_var::temp_dir().join(format!(
        "mudu-adapter-syscall-test-{}",
        mudu_sys::random::uuid_v4()
    )));
    test();
    config::reset_db_path_override_for_test();
}

fn stmt(sql: &str) -> SQLStmtText {
    SQLStmtText::new(sql.to_string())
}

#[test]
fn syscall_sqlite_backend_session_kv_sql_relation() {
    with_temp_backend(|| {
        let session = syscall::mudu_open(0).unwrap();
        let second = syscall::mudu_open_argv(&UniSessionOpenArgv::new(0)).unwrap();
        syscall::mudu_close(second).unwrap();

        // KV syscalls.
        syscall::mudu_put(session, b"sk", b"sv").unwrap();
        syscall::mudu_set(session, b"sk2", b"sv2").unwrap();
        assert_eq!(
            syscall::mudu_get(session, b"sk").unwrap(),
            Some(b"sv".to_vec())
        );
        assert!(!syscall::mudu_range(session, b"", b"z").unwrap().is_empty());

        // SQL syscalls on the same session.
        syscall::mudu_command(
            session,
            &stmt("CREATE TABLE st (id INTEGER PRIMARY KEY, v INTEGER)"),
            &(),
        )
        .unwrap();
        syscall::mudu_batch(session, &stmt("INSERT INTO st VALUES (1, 11)"), &()).unwrap();
        let rows = syscall::mudu_query::<i32>(session, &stmt("SELECT v FROM st"), &()).unwrap();
        assert_eq!(rows.next_record().unwrap(), Some(11));

        // Relation syscalls on the same table.
        let key = vec![(0u64, 1i32.to_be_bytes().to_vec())];
        assert_eq!(
            syscall::mudu_relation_get(session, "st", &key, &[1]).unwrap(),
            Some(vec![Some(11i32.to_be_bytes().to_vec())])
        );
        assert_eq!(
            syscall::mudu_relation_update(
                session,
                "st",
                &key,
                &[(1u64, 12i32.to_be_bytes().to_vec())],
                &[UniRelationDelta::add(1, 1i64.to_be_bytes().to_vec())],
            )
            .unwrap(),
            1
        );
        syscall::mudu_relation_insert(
            session,
            "st",
            &[(0u64, 5i32.to_be_bytes().to_vec())],
            &[(1u64, 51i32.to_be_bytes().to_vec())],
        )
        .unwrap();

        // fs syscalls through the local-fs emulation.
        let fd = syscall::mudu_fs_open(session, 99, "docs/a.txt", 1).unwrap();
        assert_eq!(syscall::mudu_fs_write(session, fd, b"hello").unwrap(), 5);
        // Positional write while the fd is still open for writing.
        syscall::mudu_fs_pwrite(session, fd, 0, b"H").unwrap();
        syscall::mudu_fs_fsync(session, fd).unwrap();
        assert_eq!(syscall::mudu_fs_fstat(session, fd).unwrap().length, 5);
        assert_eq!(syscall::mudu_fs_lseek(session, fd, 1, 0).unwrap(), 1);
        syscall::mudu_fs_close(session, fd).unwrap();

        // Reopen for reading.
        let fd = syscall::mudu_fs_open(session, 99, "docs/a.txt", 0).unwrap();
        assert_eq!(syscall::mudu_fs_read(session, fd, 5).unwrap(), b"Hello");
        assert_eq!(syscall::mudu_fs_pread(session, fd, 1, 3).unwrap(), b"ell");
        syscall::mudu_fs_close(session, fd).unwrap();

        assert_eq!(
            syscall::mudu_fs_stat(session, 99, "docs/a.txt")
                .unwrap()
                .length,
            5
        );
        let entries = syscall::mudu_fs_readdir(session, 99, "docs").unwrap();
        assert!(entries.iter().any(|e| e.name == "a.txt"));

        // Error paths: opening a missing entry for reading, or with an
        // unsupported access mode, fails.
        assert!(syscall::mudu_fs_open(session, 98, "missing.txt", 0).is_err());
        assert!(syscall::mudu_fs_open(session, 99, "docs/a.txt", 3).is_err());

        syscall::mudu_close(session).unwrap();
    });
}

#[test]
fn syscall_async_kv_and_sql_variants_roundtrip() {
    with_temp_backend(|| {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let session = syscall::mudu_open_async(0).await.unwrap();
            let second = syscall::mudu_open_argv_async(&UniSessionOpenArgv::new(0))
                .await
                .unwrap();
            syscall::mudu_close_async(second).await.unwrap();

            syscall::mudu_put_async(session, b"ak", b"av")
                .await
                .unwrap();
            syscall::mudu_set_async(session, b"ak2", b"av2")
                .await
                .unwrap();
            assert_eq!(
                syscall::mudu_get_async(session, b"ak").await.unwrap(),
                Some(b"av".to_vec())
            );
            assert!(
                !syscall::mudu_range_async(session, b"", b"z")
                    .await
                    .unwrap()
                    .is_empty()
            );

            syscall::mudu_command_async(
                session,
                &stmt("CREATE TABLE at (id INTEGER PRIMARY KEY, v INTEGER)"),
                &(),
            )
            .await
            .unwrap();
            syscall::mudu_batch_async(session, &stmt("INSERT INTO at VALUES (1, 21)"), &())
                .await
                .unwrap();
            let rows = syscall::mudu_query_async::<i32>(session, &stmt("SELECT v FROM at"), &())
                .await
                .unwrap();
            assert_eq!(rows.next_record().unwrap(), Some(21));

            syscall::mudu_close_async(session).await.unwrap();
        })
        .unwrap();
    });
}

#[test]
fn syscall_async_relation_and_fs_variants_roundtrip() {
    with_temp_backend(|| {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let session = syscall::mudu_open_async(0).await.unwrap();

            syscall::mudu_command_async(
                session,
                &stmt("CREATE TABLE at (id INTEGER PRIMARY KEY, v INTEGER)"),
                &(),
            )
            .await
            .unwrap();

            // Relation syscalls against the same session.
            syscall::mudu_relation_insert_async(
                session,
                "at",
                &[(0u64, 2i32.to_be_bytes().to_vec())],
                &[(1u64, 22i32.to_be_bytes().to_vec())],
            )
            .await
            .unwrap();
            assert_eq!(
                syscall::mudu_relation_update_async(
                    session,
                    "at",
                    &[(0u64, 2i32.to_be_bytes().to_vec())],
                    &[(1u64, 23i32.to_be_bytes().to_vec())],
                    &[],
                )
                .await
                .unwrap(),
                1
            );
            assert!(
                syscall::mudu_relation_get_async(
                    session,
                    "at",
                    &[(0u64, 2i32.to_be_bytes().to_vec())],
                    &[1],
                )
                .await
                .unwrap()
                .is_some()
            );

            fs_object_roundtrip_async(session).await;

            syscall::mudu_close_async(session).await.unwrap();
        })
        .unwrap();
    });
}

/// Writes, re-reads, stats and lists a single entry of fs object 7 through
/// the async fs syscalls.
async fn fs_object_roundtrip_async(session: OID) {
    let fd = syscall::mudu_fs_open_async(session, 7, "docs/f.bin", 1)
        .await
        .unwrap();
    assert_eq!(
        syscall::mudu_fs_write_async(session, fd, b"xy")
            .await
            .unwrap(),
        2
    );
    // Positional write while the fd is still open for writing.
    syscall::mudu_fs_pwrite_async(session, fd, 1, b"z")
        .await
        .unwrap();
    assert_eq!(
        syscall::mudu_fs_fstat_async(session, fd)
            .await
            .unwrap()
            .length,
        2
    );
    syscall::mudu_fs_fsync_async(session, fd).await.unwrap();
    syscall::mudu_fs_close_async(session, fd).await.unwrap();

    // Reopen for reading.
    let fd = syscall::mudu_fs_open_async(session, 7, "docs/f.bin", 0)
        .await
        .unwrap();
    assert_eq!(
        syscall::mudu_fs_read_async(session, fd, 2).await.unwrap(),
        b"xz"
    );
    assert_eq!(
        syscall::mudu_fs_lseek_async(session, fd, 0, 0)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        syscall::mudu_fs_pread_async(session, fd, 0, 2)
            .await
            .unwrap(),
        b"xz"
    );
    syscall::mudu_fs_close_async(session, fd).await.unwrap();
    assert_eq!(
        syscall::mudu_fs_stat_async(session, 7, "docs/f.bin")
            .await
            .unwrap()
            .length,
        2
    );
    let entries = syscall::mudu_fs_readdir_async(session, 7, "docs")
        .await
        .unwrap();
    assert!(entries.iter().any(|e| e.name == "f.bin"));
}

#[test]
fn local_fs_error_paths() {
    with_temp_backend(|| {
        let session = syscall::mudu_open(0).unwrap();

        // Access-mode violations on both open modes.
        let fd = syscall::mudu_fs_open(session, 97, "rw.bin", 1).unwrap();
        assert!(syscall::mudu_fs_read(session, fd, 1).is_err());
        assert!(syscall::mudu_fs_pread(session, fd, 0, 1).is_err());
        assert_eq!(syscall::mudu_fs_write(session, fd, b"abc").unwrap(), 3);
        syscall::mudu_fs_close(session, fd).unwrap();

        let fd = syscall::mudu_fs_open(session, 97, "rw.bin", 0).unwrap();
        assert!(syscall::mudu_fs_write(session, fd, b"x").is_err());
        assert!(syscall::mudu_fs_pwrite(session, fd, 0, b"x").is_err());

        // lseek: cursor-relative, end-anchored, invalid whence, out of range.
        assert_eq!(syscall::mudu_fs_lseek(session, fd, 1, 1).unwrap(), 1);
        assert_eq!(syscall::mudu_fs_lseek(session, fd, -1, 2).unwrap(), 2);
        assert!(syscall::mudu_fs_lseek(session, fd, 0, 99).is_err());
        assert!(syscall::mudu_fs_lseek(session, fd, -100, 1).is_err());
        syscall::mudu_fs_close(session, fd).unwrap();
        // A closed fd rejects further IO.
        assert!(syscall::mudu_fs_read(session, fd, 1).is_err());

        // An entry that is a directory cannot be opened for writing.
        let fd = syscall::mudu_fs_open(session, 96, "sub/dir.bin", 1).unwrap();
        syscall::mudu_fs_close(session, fd).unwrap();
        assert!(syscall::mudu_fs_open(session, 96, "sub", 1).is_err());

        // The object-root listing scans entry names; missing objects stat out.
        let entries = syscall::mudu_fs_readdir(session, 96, "").unwrap();
        assert!(entries.iter().any(|e| e.name == "sub"));
        assert!(syscall::mudu_fs_stat(session, 95, "nope").is_err());

        syscall::mudu_close(session).unwrap();
    });
}
