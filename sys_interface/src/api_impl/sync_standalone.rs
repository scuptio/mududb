use crate::fs::{FsDirEntry, FsStat};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_relation::UniRelationDelta;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

/// Execute a query against the session.
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    mudu_adapter::syscall::mudu_query(oid, sql, params)
}

/// Execute a command against the session.
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    mudu_adapter::syscall::mudu_command(oid, sql, params)
}

/// Execute a batch of statements against the session.
pub fn mudu_batch(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    mudu_adapter::syscall::mudu_batch(_oid, _sql, _params)
}

/// Open a new session against the session.
pub fn mudu_open() -> RS<OID> {
    mudu_adapter::syscall::mudu_open(0)
}

/// Open a new session with arguments against the session.
pub fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    mudu_adapter::syscall::mudu_open_argv(argv)
}

/// Close a session against the session.
pub fn mudu_close(session_id: OID) -> RS<()> {
    mudu_adapter::syscall::mudu_close(session_id)
}

/// Get a value by key against the session.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    mudu_adapter::syscall::mudu_get(session_id, key)
}

/// Store a key-value pair against the session.
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    mudu_adapter::syscall::mudu_put(session_id, key, value)
}

/// Scan a key range against the session.
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    mudu_adapter::syscall::mudu_range(session_id, start_key, end_key)
}

/// Point-read one relation row by primary key.
pub fn mudu_relation_get(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    mudu_adapter::syscall::mudu_relation_get(session_id, table, key, select)
}

/// Read-modify-write one relation row by primary key.
pub fn mudu_relation_update(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    mudu_adapter::syscall::mudu_relation_update(session_id, table, key, values, deltas)
}

/// Insert one relation row; a duplicate primary key fails.
pub fn mudu_relation_insert(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    mudu_adapter::syscall::mudu_relation_insert(session_id, table, key, values)
}

/// Open the fs object `oid` (or an entry of it) and return a file descriptor.
pub fn mudu_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    mudu_adapter::syscall::mudu_fs_open(session_id, oid, path, flags)
}

/// Close an open fs file descriptor.
pub fn mudu_fs_close(session_id: OID, fd: u32) -> RS<()> {
    mudu_adapter::syscall::mudu_fs_close(session_id, fd)
}

/// Read up to `len` bytes at the fd cursor, advancing the cursor.
pub fn mudu_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    mudu_adapter::syscall::mudu_fs_read(session_id, fd, len)
}

/// Write `data` at the fd cursor, advancing the cursor; returns bytes written.
pub fn mudu_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    mudu_adapter::syscall::mudu_fs_write(session_id, fd, data)
}

/// Read up to `len` bytes at `offset` without moving the fd cursor.
pub fn mudu_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    mudu_adapter::syscall::mudu_fs_pread(session_id, fd, offset, len)
}

/// Write `data` at `offset` without moving the fd cursor.
pub fn mudu_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    mudu_adapter::syscall::mudu_fs_pwrite(session_id, fd, offset, data)
}

/// Move the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new cursor.
pub fn mudu_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    mudu_adapter::syscall::mudu_fs_lseek(session_id, fd, offset, whence)
}

/// Stat an open fs file descriptor.
pub fn mudu_fs_fstat(session_id: OID, fd: u32) -> RS<FsStat> {
    mudu_adapter::syscall::mudu_fs_fstat(session_id, fd).map(FsStat::from)
}

/// Stat the fs object `oid` (or an entry of it) without opening an fd.
pub fn mudu_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<FsStat> {
    mudu_adapter::syscall::mudu_fs_stat(session_id, oid, path).map(FsStat::from)
}

/// Flush a write fd's content to durable storage.
pub fn mudu_fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    mudu_adapter::syscall::mudu_fs_fsync(session_id, fd)
}

/// List the entries of an fs object directory.
pub fn mudu_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<FsDirEntry>> {
    mudu_adapter::syscall::mudu_fs_readdir(session_id, oid, path)
        .map(|ents| ents.into_iter().map(FsDirEntry::from).collect())
}
