use crate::fs::{FsDirEntry, FsStat};
use crate::host::{
    async_invoke_host_batch, async_invoke_host_close, async_invoke_host_command,
    async_invoke_host_fs_close, async_invoke_host_fs_fstat, async_invoke_host_fs_fsync,
    async_invoke_host_fs_lseek, async_invoke_host_fs_open, async_invoke_host_fs_pread,
    async_invoke_host_fs_pwrite, async_invoke_host_fs_read, async_invoke_host_fs_readdir,
    async_invoke_host_fs_stat, async_invoke_host_fs_write, async_invoke_host_open,
    async_invoke_host_query, async_invoke_host_relation_get, async_invoke_host_relation_insert,
    async_invoke_host_relation_update, async_invoke_host_session_get,
    async_invoke_host_session_put, async_invoke_host_session_range,
};
use crate::inner_component_async::mududb::async_api::system;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_relation::UniRelationDelta;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

wit_bindgen::generate!({
    path:"wit/async",
    world: "async-api",
    async: true,    // all bindings are async
});

/// Forward a `query` call to the component-model host interface.
pub async fn inner_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    async_invoke_host_query(oid, sql, params, async |param| {
        Ok(system::query(param).await)
    })
    .await
}

/// Forward a `command` call to the component-model host interface.
pub async fn inner_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    async_invoke_host_command(oid, sql, params, async |param| {
        Ok(system::command(param).await)
    })
    .await
}

/// Forward a `batch` call to the component-model host interface.
pub async fn inner_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    async_invoke_host_batch(oid, sql, params, async |param| {
        Ok(system::batch(param).await)
    })
    .await
}

/// Forward a `open` call to the component-model host interface.
pub async fn inner_open() -> RS<OID> {
    async_invoke_host_open(async |param| Ok(system::open(param).await)).await
}

/// Forward a `open argv` call to the component-model host interface.
pub async fn inner_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    crate::host::async_invoke_host_open_argv(argv, async |param| Ok(system::open(param).await))
        .await
}

/// Forward a `close` call to the component-model host interface.
pub async fn inner_close(session_id: OID) -> RS<()> {
    async_invoke_host_close(session_id, async |param| Ok(system::close(param).await)).await
}

/// Forward a `get` call to the component-model host interface.
pub async fn inner_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    async_invoke_host_session_get(session_id, key, async |param| Ok(system::get(param).await)).await
}

/// Forward a `put` call to the component-model host interface.
pub async fn inner_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    async_invoke_host_session_put(session_id, key, value, async |param| {
        Ok(system::put(param).await)
    })
    .await
}

/// Forward a `range` call to the component-model host interface.
pub async fn inner_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    async_invoke_host_session_range(session_id, start_key, end_key, async |param| {
        Ok(system::range(param).await)
    })
    .await
}

/// Forward a `relation-get` call to the component-model host interface.
pub async fn inner_relation_get(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    async_invoke_host_relation_get(session_id, table, key, select, async |param| {
        Ok(system::relation_get(param).await)
    })
    .await
}

/// Forward a `relation-update` call to the component-model host interface.
pub async fn inner_relation_update(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    async_invoke_host_relation_update(session_id, table, key, values, deltas, async |param| {
        Ok(system::relation_update(param).await)
    })
    .await
}

/// Forward a `relation-insert` call to the component-model host interface.
pub async fn inner_relation_insert(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    async_invoke_host_relation_insert(session_id, table, key, values, async |param| {
        Ok(system::relation_insert(param).await)
    })
    .await
}

/// Forward a `fs open` call to the component-model host interface.
pub async fn inner_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    async_invoke_host_fs_open(session_id, oid, path, flags, async |param| {
        Ok(system::fs_open(param).await)
    })
    .await
}

/// Forward a `fs close` call to the component-model host interface.
pub async fn inner_fs_close(session_id: OID, fd: u32) -> RS<()> {
    async_invoke_host_fs_close(session_id, fd, async |param| {
        Ok(system::fs_close(param).await)
    })
    .await
}

/// Forward a `fs read` call to the component-model host interface.
pub async fn inner_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    async_invoke_host_fs_read(session_id, fd, len, async |param| {
        Ok(system::fs_read(param).await)
    })
    .await
}

/// Forward a `fs write` call to the component-model host interface.
pub async fn inner_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    async_invoke_host_fs_write(session_id, fd, data, async |param| {
        Ok(system::fs_write(param).await)
    })
    .await
}

/// Forward a `fs pread` call to the component-model host interface.
pub async fn inner_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    async_invoke_host_fs_pread(session_id, fd, offset, len, async |param| {
        Ok(system::fs_pread(param).await)
    })
    .await
}

/// Forward a `fs pwrite` call to the component-model host interface.
pub async fn inner_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    async_invoke_host_fs_pwrite(session_id, fd, offset, data, async |param| {
        Ok(system::fs_pwrite(param).await)
    })
    .await
}

/// Forward a `fs lseek` call to the component-model host interface.
pub async fn inner_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    async_invoke_host_fs_lseek(session_id, fd, offset, whence, async |param| {
        Ok(system::fs_lseek(param).await)
    })
    .await
}

/// Forward a `fs fstat` call to the component-model host interface.
pub async fn inner_fs_fstat(session_id: OID, fd: u32) -> RS<FsStat> {
    async_invoke_host_fs_fstat(session_id, fd, async |param| {
        Ok(system::fs_fstat(param).await)
    })
    .await
}

/// Forward a `fs stat` call to the component-model host interface.
pub async fn inner_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<FsStat> {
    async_invoke_host_fs_stat(session_id, oid, path, async |param| {
        Ok(system::fs_stat(param).await)
    })
    .await
}

/// Forward a `fs fsync` call to the component-model host interface.
pub async fn inner_fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    async_invoke_host_fs_fsync(session_id, fd, async |param| {
        Ok(system::fs_fsync(param).await)
    })
    .await
}

/// Forward a `fs readdir` call to the component-model host interface.
pub async fn inner_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<FsDirEntry>> {
    async_invoke_host_fs_readdir(session_id, oid, path, async |param| {
        Ok(system::fs_readdir(param).await)
    })
    .await
}
