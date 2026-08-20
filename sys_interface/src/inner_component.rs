use crate::fs::{FsDirEntry, FsStat};
use crate::host::{
    invoke_host_batch, invoke_host_close, invoke_host_command, invoke_host_fs_close,
    invoke_host_fs_fstat, invoke_host_fs_fsync, invoke_host_fs_lseek, invoke_host_fs_open,
    invoke_host_fs_pread, invoke_host_fs_pwrite, invoke_host_fs_read, invoke_host_fs_readdir,
    invoke_host_fs_stat, invoke_host_fs_write, invoke_host_open, invoke_host_query,
    invoke_host_relation_get, invoke_host_relation_insert, invoke_host_relation_update,
    invoke_host_session_get, invoke_host_session_put, invoke_host_session_range,
};
use crate::inner_component::mududb::api::system;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_relation::UniRelationDelta;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

wit_bindgen::generate!({
    path:"wit/sync",
    world:"api"
});

/// Forward a `query` call to the component-model host interface.
pub fn inner_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    invoke_host_query(oid, sql, params, |param| Ok(system::query(&param)))
}

/// Forward a `command` call to the component-model host interface.
pub fn inner_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    invoke_host_command(oid, sql, params, |param| Ok(system::command(&param)))
}

/// Forward a `batch` call to the component-model host interface.
pub fn inner_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    invoke_host_batch(oid, sql, params, |param| Ok(system::batch(&param)))
}

/// Forward a `open` call to the component-model host interface.
pub fn inner_open() -> RS<OID> {
    invoke_host_open(|param| Ok(system::open(&param)))
}

/// Forward a `open argv` call to the component-model host interface.
pub fn inner_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    crate::host::invoke_host_open_argv(argv, |param| Ok(system::open(&param)))
}

/// Forward a `close` call to the component-model host interface.
pub fn inner_close(session_id: OID) -> RS<()> {
    invoke_host_close(session_id, |param| Ok(system::close(&param)))
}

/// Forward a `get` call to the component-model host interface.
pub fn inner_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    invoke_host_session_get(session_id, key, |param| Ok(system::get(&param)))
}

/// Forward a `put` call to the component-model host interface.
pub fn inner_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    invoke_host_session_put(session_id, key, value, |param| Ok(system::put(&param)))
}

/// Forward a `range` call to the component-model host interface.
pub fn inner_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    invoke_host_session_range(session_id, start_key, end_key, |param| {
        Ok(system::range(&param))
    })
}

/// Forward a `relation-get` call to the component-model host interface.
pub fn inner_relation_get(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    invoke_host_relation_get(session_id, table, key, select, |param| {
        Ok(system::relation_get(&param))
    })
}

/// Forward a `relation-update` call to the component-model host interface.
pub fn inner_relation_update(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    invoke_host_relation_update(session_id, table, key, values, deltas, |param| {
        Ok(system::relation_update(&param))
    })
}

/// Forward a `relation-insert` call to the component-model host interface.
pub fn inner_relation_insert(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    invoke_host_relation_insert(session_id, table, key, values, |param| {
        Ok(system::relation_insert(&param))
    })
}

/// Forward a `fs open` call to the component-model host interface.
pub fn inner_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    invoke_host_fs_open(session_id, oid, path, flags, |param| {
        Ok(system::fs_open(&param))
    })
}

/// Forward a `fs close` call to the component-model host interface.
pub fn inner_fs_close(session_id: OID, fd: u32) -> RS<()> {
    invoke_host_fs_close(session_id, fd, |param| Ok(system::fs_close(&param)))
}

/// Forward a `fs read` call to the component-model host interface.
pub fn inner_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    invoke_host_fs_read(session_id, fd, len, |param| Ok(system::fs_read(&param)))
}

/// Forward a `fs write` call to the component-model host interface.
pub fn inner_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    invoke_host_fs_write(session_id, fd, data, |param| Ok(system::fs_write(&param)))
}

/// Forward a `fs pread` call to the component-model host interface.
pub fn inner_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    invoke_host_fs_pread(session_id, fd, offset, len, |param| {
        Ok(system::fs_pread(&param))
    })
}

/// Forward a `fs pwrite` call to the component-model host interface.
pub fn inner_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    invoke_host_fs_pwrite(session_id, fd, offset, data, |param| {
        Ok(system::fs_pwrite(&param))
    })
}

/// Forward a `fs lseek` call to the component-model host interface.
pub fn inner_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    invoke_host_fs_lseek(session_id, fd, offset, whence, |param| {
        Ok(system::fs_lseek(&param))
    })
}

/// Forward a `fs fstat` call to the component-model host interface.
pub fn inner_fs_fstat(session_id: OID, fd: u32) -> RS<FsStat> {
    invoke_host_fs_fstat(session_id, fd, |param| Ok(system::fs_fstat(&param)))
}

/// Forward a `fs stat` call to the component-model host interface.
pub fn inner_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<FsStat> {
    invoke_host_fs_stat(session_id, oid, path, |param| Ok(system::fs_stat(&param)))
}

/// Forward a `fs fsync` call to the component-model host interface.
pub fn inner_fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    invoke_host_fs_fsync(session_id, fd, |param| Ok(system::fs_fsync(&param)))
}

/// Forward a `fs readdir` call to the component-model host interface.
pub fn inner_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<FsDirEntry>> {
    invoke_host_fs_readdir(session_id, oid, path, |param| {
        Ok(system::fs_readdir(&param))
    })
}
