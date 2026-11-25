use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_relation::UniRelationDelta;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::object::Object;
use mudu_contract::database::object_set::ObjectSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use sys_interface::fs::{FsDirEntry, FsStat};

/// Execute a query against the session.
pub async fn mudu_query<R: Object>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<ObjectSet<R>> {
    mudu_adapter::syscall::mudu_query_async(oid, sql, params).await
}

/// Execute a command against the session.
pub async fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    mudu_adapter::syscall::mudu_command_async(oid, sql, params).await
}

/// Execute a batch of statements against the session.
pub async fn mudu_batch(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    mudu_adapter::syscall::mudu_batch_async(_oid, _sql, _params).await
}

/// Open a new session against the session.
pub async fn mudu_open() -> RS<OID> {
    mudu_adapter::syscall::mudu_open_async(0).await
}

/// Open a new session with arguments against the session.
pub async fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    mudu_adapter::syscall::mudu_open_argv_async(argv).await
}

/// Close a session against the session.
pub async fn mudu_close(session_id: OID) -> RS<()> {
    mudu_adapter::syscall::mudu_close_async(session_id).await
}

/// Get a value by key against the session.
pub async fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    mudu_adapter::syscall::mudu_get_async(session_id, key).await
}

/// Store a key-value pair against the session.
pub async fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    mudu_adapter::syscall::mudu_put_async(session_id, key, value).await
}

/// Scan a key range against the session.
pub async fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    mudu_adapter::syscall::mudu_range_async(session_id, start_key, end_key).await
}

/// Point-read one relation row by primary key.
pub async fn mudu_relation_get(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    mudu_adapter::syscall::mudu_relation_get_async(session_id, table, key, select).await
}

/// Read-modify-write one relation row by primary key.
pub async fn mudu_relation_update(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    mudu_adapter::syscall::mudu_relation_update_async(session_id, table, key, values, deltas).await
}

/// Insert one relation row; a duplicate primary key fails.
pub async fn mudu_relation_insert(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    mudu_adapter::syscall::mudu_relation_insert_async(session_id, table, key, values).await
}

/// Open the fs object `oid` (or an entry of it) and return a file descriptor.
pub async fn mudu_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    mudu_adapter::syscall::mudu_fs_open_async(session_id, oid, path, flags).await
}

/// Close an open fs file descriptor.
pub async fn mudu_fs_close(session_id: OID, fd: u32) -> RS<()> {
    mudu_adapter::syscall::mudu_fs_close_async(session_id, fd).await
}

/// Read up to `len` bytes at the fd cursor, advancing the cursor.
pub async fn mudu_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    mudu_adapter::syscall::mudu_fs_read_async(session_id, fd, len).await
}

/// Write `data` at the fd cursor, advancing the cursor; returns bytes written.
pub async fn mudu_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    mudu_adapter::syscall::mudu_fs_write_async(session_id, fd, data).await
}

/// Read up to `len` bytes at `offset` without moving the fd cursor.
pub async fn mudu_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    mudu_adapter::syscall::mudu_fs_pread_async(session_id, fd, offset, len).await
}

/// Write `data` at `offset` without moving the fd cursor.
pub async fn mudu_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    mudu_adapter::syscall::mudu_fs_pwrite_async(session_id, fd, offset, data).await
}

/// Move the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new cursor.
pub async fn mudu_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    mudu_adapter::syscall::mudu_fs_lseek_async(session_id, fd, offset, whence).await
}

/// Stat an open fs file descriptor.
pub async fn mudu_fs_fstat(session_id: OID, fd: u32) -> RS<FsStat> {
    mudu_adapter::syscall::mudu_fs_fstat_async(session_id, fd)
        .await
        .map(FsStat::from)
}

/// Stat the fs object `oid` (or an entry of it) without opening an fd.
pub async fn mudu_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<FsStat> {
    mudu_adapter::syscall::mudu_fs_stat_async(session_id, oid, path)
        .await
        .map(FsStat::from)
}

/// Flush a write fd's content to durable storage.
pub async fn mudu_fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    mudu_adapter::syscall::mudu_fs_fsync_async(session_id, fd).await
}

/// List the entries of an fs object directory.
pub async fn mudu_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<FsDirEntry>> {
    mudu_adapter::syscall::mudu_fs_readdir_async(session_id, oid, path)
        .await
        .map(|ents| ents.into_iter().map(FsDirEntry::from).collect())
}

#[cfg(all(test, not(miri)))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
    use mudu_contract::database::sql_stmt_text::SQLStmtText;

    fn run_adapter_test<F, Fut>(name: &str, f: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + 'static,
    {
        let _guard = mudu_adapter::config::test_lock()
            .lock()
            .expect("test lock poisoned");
        let db_path = mudu_sys::env_var::temp_dir().join(format!(
            "sys_interface_async_impl_{name}_{}",
            mudu_sys::random::uuid_v4()
        ));
        mudu_adapter::syscall::set_db_path(&db_path);
        mudu_sys::task::async_::block_on_tokio_current_thread(async move { f().await }).unwrap();
        mudu_adapter::config::reset_db_path_override_for_test();
        let _ = mudu_sys::fs::sync::sync_remove_file(&db_path);
    }

    #[test]
    fn standalone_async_facade_full_roundtrip() {
        run_adapter_test("full", || async {
            let session = mudu_open().await.unwrap();
            let second = mudu_open_argv(&UniSessionOpenArgv::new(0)).await.unwrap();
            mudu_close(second).await.unwrap();

            // SQL forwarders.
            mudu_command(
                session,
                &SQLStmtText::new("CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER)".to_string()),
                &(),
            )
            .await
            .unwrap();
            mudu_batch(
                session,
                &SQLStmtText::new("INSERT INTO t VALUES (1, 11)".to_string()),
                &(),
            )
            .await
            .unwrap();
            let rows = mudu_query::<i32>(
                session,
                &SQLStmtText::new("SELECT v FROM t".to_string()),
                &(),
            )
            .await
            .unwrap();
            assert_eq!(rows.next_record().unwrap(), Some(11));

            // KV forwarders.
            mudu_put(session, b"k", b"v").await.unwrap();
            assert_eq!(mudu_get(session, b"k").await.unwrap(), Some(b"v".to_vec()));
            assert!(!mudu_range(session, b"", b"z").await.unwrap().is_empty());

            // Relation forwarders.
            let key = vec![(0u64, 1i32.to_be_bytes().to_vec())];
            assert_eq!(
                mudu_relation_get(session, "t", &key, &[1]).await.unwrap(),
                Some(vec![Some(11i32.to_be_bytes().to_vec())])
            );
            assert_eq!(
                mudu_relation_update(
                    session,
                    "t",
                    &key,
                    &[(1u64, 12i32.to_be_bytes().to_vec())],
                    &[UniRelationDelta::add(1, 1i64.to_be_bytes().to_vec())],
                )
                .await
                .unwrap(),
                1
            );
            mudu_relation_insert(
                session,
                "t",
                &[(0u64, 5i32.to_be_bytes().to_vec())],
                &[(1u64, 51i32.to_be_bytes().to_vec())],
            )
            .await
            .unwrap();

            // fs forwarders through the local-fs emulation.
            let fd = mudu_fs_open(session, 99, "docs/a.txt", 1).await.unwrap();
            assert_eq!(mudu_fs_write(session, fd, b"hello").await.unwrap(), 5);
            mudu_fs_pwrite(session, fd, 0, b"H").await.unwrap();
            mudu_fs_fsync(session, fd).await.unwrap();
            assert_eq!(mudu_fs_fstat(session, fd).await.unwrap().length, 5);
            assert_eq!(mudu_fs_lseek(session, fd, 1, 0).await.unwrap(), 1);
            mudu_fs_close(session, fd).await.unwrap();

            let fd = mudu_fs_open(session, 99, "docs/a.txt", 0).await.unwrap();
            assert_eq!(mudu_fs_read(session, fd, 5).await.unwrap(), b"Hello");
            assert_eq!(mudu_fs_pread(session, fd, 1, 3).await.unwrap(), b"ell");
            mudu_fs_close(session, fd).await.unwrap();

            assert_eq!(
                mudu_fs_stat(session, 99, "docs/a.txt")
                    .await
                    .unwrap()
                    .length,
                5
            );
            let entries = mudu_fs_readdir(session, 99, "docs").await.unwrap();
            assert!(entries.iter().any(|e| e.name == "a.txt"));

            mudu_close(session).await.unwrap();
        });
    }
}
