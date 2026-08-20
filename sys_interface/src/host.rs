use crate::fs;
use crate::fs::{FsDirEntry, FsStat};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::codec::syscall_payload;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_relation::UniRelationDelta;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::result_set::ResultSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::SMutex;
use std::sync::Arc;

/// Invoke the host `command` operation.
pub fn invoke_host_command<F>(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams, f: F) -> RS<u64>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::command_invoke::serialize_command_param(oid, sql, params)?;
    let result = f(param_binary)?;
    let affected_rows = mudu_binding::system::command_invoke::deserialize_command_result(&result)?;
    Ok(affected_rows)
}

/// Invoke the host `batch` operation.
pub fn invoke_host_batch<F>(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams, f: F) -> RS<u64>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    invoke_host_command(oid, sql, params, f)
}

/// Invoke the host `query` operation.
pub fn invoke_host_query<R: Entity, F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::query_invoke::serialize_query_dyn_param(oid, sql, params)?;
    let result = f(param_binary)?;
    let (result_batch, tuple_desc) =
        mudu_binding::system::query_invoke::deserialize_query_result(&result)?;
    let record_set = RecordSet::<R>::new(
        Arc::new(ResultSetWrapper::new(result_batch)),
        Arc::new(tuple_desc),
    );
    Ok(record_set)
}

/// Serialize session get param parameters.
pub fn serialize_session_get_param(session_id: OID, key: &[u8]) -> Vec<u8> {
    syscall_payload::encode_get_request(session_id.into(), key)
}

/// Deserialize session get param parameters/results.
pub fn deserialize_session_get_param(input: &[u8]) -> RS<(OID, Vec<u8>)> {
    syscall_payload::decode_get_request(input).map(|(oid, key)| (oid.to_oid(), key))
}

/// Serialize get result parameters.
pub fn serialize_get_result(value: Option<&[u8]>) -> Vec<u8> {
    syscall_payload::encode_get_result(&Ok(value.map(<[u8]>::to_vec)))
}

/// Deserialize get result parameters/results.
pub fn deserialize_get_result(input: &[u8]) -> RS<Option<Vec<u8>>> {
    syscall_payload::decode_get_result(input)
}

/// Serialize session put param parameters.
pub fn serialize_session_put_param(session_id: OID, key: &[u8], value: &[u8]) -> Vec<u8> {
    syscall_payload::encode_put_request(session_id.into(), key, value)
}

/// Deserialize session put param parameters/results.
pub fn deserialize_session_put_param(input: &[u8]) -> RS<(OID, Vec<u8>, Vec<u8>)> {
    syscall_payload::decode_put_request(input).map(|(oid, key, value)| (oid.to_oid(), key, value))
}

/// Serialize put result parameters.
pub fn serialize_put_result() -> Vec<u8> {
    syscall_payload::encode_put_result(&Ok(()))
}

/// Deserialize put result parameters/results.
pub fn deserialize_put_result(input: &[u8]) -> RS<()> {
    syscall_payload::decode_put_result(input)
}

/// Serialize session range param parameters.
pub fn serialize_session_range_param(session_id: OID, start_key: &[u8], end_key: &[u8]) -> Vec<u8> {
    syscall_payload::encode_range_request(session_id.into(), start_key, end_key)
}

/// Deserialize session range param parameters/results.
pub fn deserialize_session_range_param(input: &[u8]) -> RS<(OID, Vec<u8>, Vec<u8>)> {
    syscall_payload::decode_range_request(input)
        .map(|(oid, start_key, end_key)| (oid.to_oid(), start_key, end_key))
}

/// Serialize open param parameters.
pub fn serialize_open_param() -> Vec<u8> {
    syscall_payload::encode_open_request(UniOid::default())
}

/// Serialize open argv param parameters.
pub fn serialize_open_argv_param(argv: &UniSessionOpenArgv) -> Vec<u8> {
    syscall_payload::encode_open_request(argv.worker_id.clone())
}

/// Deserialize open param parameters/results.
pub fn deserialize_open_param(input: &[u8]) -> RS<UniSessionOpenArgv> {
    syscall_payload::decode_open_request(input).map(|worker_id| UniSessionOpenArgv { worker_id })
}

/// Serialize open result parameters.
pub fn serialize_open_result(session_id: OID) -> Vec<u8> {
    syscall_payload::encode_open_result(&Ok(session_id))
}

/// Deserialize open result parameters/results.
pub fn deserialize_open_result(input: &[u8]) -> RS<OID> {
    syscall_payload::decode_open_result(input)
}

/// Serialize close param parameters.
pub fn serialize_close_param(session_id: OID) -> Vec<u8> {
    syscall_payload::encode_close_request(session_id.into())
}

/// Deserialize close param parameters/results.
pub fn deserialize_close_param(input: &[u8]) -> RS<OID> {
    syscall_payload::decode_close_request(input).map(|oid| oid.to_oid())
}

/// Serialize close result parameters.
pub fn serialize_close_result() -> Vec<u8> {
    syscall_payload::encode_close_result(&Ok(()))
}

/// Deserialize close result parameters/results.
pub fn deserialize_close_result(input: &[u8]) -> RS<()> {
    syscall_payload::decode_close_result(input)
}

/// Serialize range result parameters.
pub fn serialize_range_result(items: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    syscall_payload::encode_range_result(&Ok(items.to_vec()))
}

/// Deserialize range result parameters/results.
pub fn deserialize_range_result(input: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    syscall_payload::decode_range_result(input)
}

/// Invoke the host `open` operation.
pub fn invoke_host_open<F>(f: F) -> RS<OID>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_param();
    let result = f(param_binary)?;
    deserialize_open_result(&result)
}

/// Invoke the host `open argv` operation.
pub fn invoke_host_open_argv<F>(argv: &UniSessionOpenArgv, f: F) -> RS<OID>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_argv_param(argv);
    let result = f(param_binary)?;
    deserialize_open_result(&result)
}

/// Invoke the host `close` operation.
pub fn invoke_host_close<F>(session_id: OID, f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_close_param(session_id);
    let result = f(param_binary)?;
    deserialize_close_result(&result)
}

/// Invoke the host `session get` operation.
pub fn invoke_host_session_get<F>(session_id: OID, key: &[u8], f: F) -> RS<Option<Vec<u8>>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_get_param(session_id, key);
    let result = f(param_binary)?;
    deserialize_get_result(&result)
}

/// Invoke the host `session put` operation.
pub fn invoke_host_session_put<F>(session_id: OID, key: &[u8], value: &[u8], f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_put_param(session_id, key, value);
    let result = f(param_binary)?;
    deserialize_put_result(&result)
}

/// Invoke the host `session range` operation.
pub fn invoke_host_session_range<F>(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
    f: F,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_range_param(session_id, start_key, end_key);
    let result = f(param_binary)?;
    deserialize_range_result(&result)
}

/// Serialize relation get param parameters.
pub fn serialize_relation_get_param(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> Vec<u8> {
    let key_refs = key
        .iter()
        .map(|(attr, datum)| (*attr, datum.as_slice()))
        .collect::<Vec<_>>();
    syscall_payload::encode_relation_get_request(session_id.into(), table, &key_refs, select)
}

/// Deserialize relation get results.
pub fn deserialize_relation_get_result(input: &[u8]) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    syscall_payload::decode_relation_get_result(input)
}

/// Serialize relation update param parameters.
pub fn serialize_relation_update_param(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> Vec<u8> {
    let key_refs = key
        .iter()
        .map(|(attr, datum)| (*attr, datum.as_slice()))
        .collect::<Vec<_>>();
    let value_refs = values
        .iter()
        .map(|(attr, datum)| (*attr, datum.as_slice()))
        .collect::<Vec<_>>();
    let delta_refs = deltas
        .iter()
        .map(|delta| (delta.attr, delta.op, delta.datum.as_slice()))
        .collect::<Vec<_>>();
    syscall_payload::encode_relation_update_request(
        session_id.into(),
        table,
        &key_refs,
        &value_refs,
        &delta_refs,
    )
}

/// Deserialize relation update results.
pub fn deserialize_relation_update_result(input: &[u8]) -> RS<u64> {
    syscall_payload::decode_relation_update_result(input)
}

/// Serialize relation insert param parameters.
pub fn serialize_relation_insert_param(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> Vec<u8> {
    let key_refs = key
        .iter()
        .map(|(attr, datum)| (*attr, datum.as_slice()))
        .collect::<Vec<_>>();
    let value_refs = values
        .iter()
        .map(|(attr, datum)| (*attr, datum.as_slice()))
        .collect::<Vec<_>>();
    syscall_payload::encode_relation_insert_request(
        session_id.into(),
        table,
        &key_refs,
        &value_refs,
    )
}

/// Deserialize relation insert results.
pub fn deserialize_relation_insert_result(input: &[u8]) -> RS<()> {
    syscall_payload::decode_relation_insert_result(input)
}

/// Invoke the host `relation-get` operation.
pub fn invoke_host_relation_get<F>(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
    f: F,
) -> RS<Option<Vec<Option<Vec<u8>>>>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_relation_get_param(session_id, table, key, select);
    let result = f(param_binary)?;
    deserialize_relation_get_result(&result)
}

/// Invoke the host `relation-update` operation.
pub fn invoke_host_relation_update<F>(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
    f: F,
) -> RS<u64>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_relation_update_param(session_id, table, key, values, deltas);
    let result = f(param_binary)?;
    deserialize_relation_update_result(&result)
}

/// Invoke the host `relation-insert` operation.
pub fn invoke_host_relation_insert<F>(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    f: F,
) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_relation_insert_param(session_id, table, key, values);
    let result = f(param_binary)?;
    deserialize_relation_insert_result(&result)
}

/// Invoke the host `fs open` operation.
pub fn invoke_host_fs_open<F>(session_id: OID, oid: OID, path: &str, flags: u32, f: F) -> RS<u32>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_open_param(session_id, oid, path, flags);
    let result = f(param_binary)?;
    fs::deserialize_fs_open_result(&result)
}

/// Invoke the host `fs close` operation.
pub fn invoke_host_fs_close<F>(session_id: OID, fd: u32, f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_close_param(session_id, fd);
    let result = f(param_binary)?;
    fs::deserialize_fs_close_result(&result)
}

/// Invoke the host `fs read` operation.
pub fn invoke_host_fs_read<F>(session_id: OID, fd: u32, len: u32, f: F) -> RS<Vec<u8>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_read_param(session_id, fd, len);
    let result = f(param_binary)?;
    fs::deserialize_fs_read_result(&result)
}

/// Invoke the host `fs write` operation.
pub fn invoke_host_fs_write<F>(session_id: OID, fd: u32, data: &[u8], f: F) -> RS<u32>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_write_param(session_id, fd, data);
    let result = f(param_binary)?;
    fs::deserialize_fs_write_result(&result)
}

/// Invoke the host `fs pread` operation.
pub fn invoke_host_fs_pread<F>(session_id: OID, fd: u32, offset: u64, len: u32, f: F) -> RS<Vec<u8>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_pread_param(session_id, fd, offset, len);
    let result = f(param_binary)?;
    fs::deserialize_fs_pread_result(&result)
}

/// Invoke the host `fs pwrite` operation.
pub fn invoke_host_fs_pwrite<F>(session_id: OID, fd: u32, offset: u64, data: &[u8], f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_pwrite_param(session_id, fd, offset, data);
    let result = f(param_binary)?;
    fs::deserialize_fs_pwrite_result(&result)
}

/// Invoke the host `fs lseek` operation.
pub fn invoke_host_fs_lseek<F>(session_id: OID, fd: u32, offset: i64, whence: u32, f: F) -> RS<u64>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_lseek_param(session_id, fd, offset, whence);
    let result = f(param_binary)?;
    fs::deserialize_fs_lseek_result(&result)
}

/// Invoke the host `fs fstat` operation.
pub fn invoke_host_fs_fstat<F>(session_id: OID, fd: u32, f: F) -> RS<FsStat>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_fstat_param(session_id, fd);
    let result = f(param_binary)?;
    fs::deserialize_fs_fstat_result(&result)
}

/// Invoke the host `fs stat` operation.
pub fn invoke_host_fs_stat<F>(session_id: OID, oid: OID, path: &str, f: F) -> RS<FsStat>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_stat_param(session_id, oid, path);
    let result = f(param_binary)?;
    fs::deserialize_fs_stat_result(&result)
}

/// Invoke the host `fs fsync` operation.
pub fn invoke_host_fs_fsync<F>(session_id: OID, fd: u32, f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_fsync_param(session_id, fd);
    let result = f(param_binary)?;
    fs::deserialize_fs_fsync_result(&result)
}

/// Invoke the host `fs readdir` operation.
pub fn invoke_host_fs_readdir<F>(session_id: OID, oid: OID, path: &str, f: F) -> RS<Vec<FsDirEntry>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_readdir_param(session_id, oid, path);
    let result = f(param_binary)?;
    fs::deserialize_fs_readdir_result(&result)
}

/// Asynchronously invoke the host `command` operation.
pub async fn async_invoke_host_command<F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<u64>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::command_invoke::serialize_command_param(oid, sql, params)?;
    let result = f(param_binary).await?;
    let affected_rows = mudu_binding::system::command_invoke::deserialize_command_result(&result)?;
    Ok(affected_rows)
}

/// Asynchronously invoke the host `batch` operation.
pub async fn async_invoke_host_batch<F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<u64>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    async_invoke_host_command(oid, sql, params, f).await
}

/// Asynchronously invoke the host `query` operation.
pub async fn async_invoke_host_query<R: Entity, F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::query_invoke::serialize_query_dyn_param(oid, sql, params)?;
    let result = f(param_binary).await?;
    let (result_batch, tuple_desc) =
        mudu_binding::system::query_invoke::deserialize_query_result(&result)?;
    let record_set = RecordSet::<R>::new(
        Arc::new(ResultSetWrapper::new(result_batch)),
        Arc::new(tuple_desc),
    );
    Ok(record_set)
}

/// Asynchronously invoke the host `open` operation.
pub async fn async_invoke_host_open<F>(f: F) -> RS<OID>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_param();
    let result = f(param_binary).await?;
    deserialize_open_result(&result)
}

/// Asynchronously invoke the host `open argv` operation.
pub async fn async_invoke_host_open_argv<F>(argv: &UniSessionOpenArgv, f: F) -> RS<OID>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_argv_param(argv);
    let result = f(param_binary).await?;
    deserialize_open_result(&result)
}

/// Asynchronously invoke the host `close` operation.
pub async fn async_invoke_host_close<F>(session_id: OID, f: F) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_close_param(session_id);
    let result = f(param_binary).await?;
    deserialize_close_result(&result)
}

/// Asynchronously invoke the host `session get` operation.
pub async fn async_invoke_host_session_get<F>(
    session_id: OID,
    key: &[u8],
    f: F,
) -> RS<Option<Vec<u8>>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_get_param(session_id, key);
    let result = f(param_binary).await?;
    deserialize_get_result(&result)
}

/// Asynchronously invoke the host `session put` operation.
pub async fn async_invoke_host_session_put<F>(
    session_id: OID,
    key: &[u8],
    value: &[u8],
    f: F,
) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_put_param(session_id, key, value);
    let result = f(param_binary).await?;
    deserialize_put_result(&result)
}

/// Asynchronously invoke the host `session range` operation.
pub async fn async_invoke_host_session_range<F>(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
    f: F,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_range_param(session_id, start_key, end_key);
    let result = f(param_binary).await?;
    deserialize_range_result(&result)
}

/// Asynchronously invoke the host `relation-get` operation.
pub async fn async_invoke_host_relation_get<F>(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
    f: F,
) -> RS<Option<Vec<Option<Vec<u8>>>>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_relation_get_param(session_id, table, key, select);
    let result = f(param_binary).await?;
    deserialize_relation_get_result(&result)
}

/// Asynchronously invoke the host `relation-update` operation.
pub async fn async_invoke_host_relation_update<F>(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
    f: F,
) -> RS<u64>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_relation_update_param(session_id, table, key, values, deltas);
    let result = f(param_binary).await?;
    deserialize_relation_update_result(&result)
}

/// Asynchronously invoke the host `relation-insert` operation.
pub async fn async_invoke_host_relation_insert<F>(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    f: F,
) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_relation_insert_param(session_id, table, key, values);
    let result = f(param_binary).await?;
    deserialize_relation_insert_result(&result)
}

/// Asynchronously invoke the host `fs open` operation.
pub async fn async_invoke_host_fs_open<F>(
    session_id: OID,
    oid: OID,
    path: &str,
    flags: u32,
    f: F,
) -> RS<u32>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_open_param(session_id, oid, path, flags);
    let result = f(param_binary).await?;
    fs::deserialize_fs_open_result(&result)
}

/// Asynchronously invoke the host `fs close` operation.
pub async fn async_invoke_host_fs_close<F>(session_id: OID, fd: u32, f: F) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_close_param(session_id, fd);
    let result = f(param_binary).await?;
    fs::deserialize_fs_close_result(&result)
}

/// Asynchronously invoke the host `fs read` operation.
pub async fn async_invoke_host_fs_read<F>(session_id: OID, fd: u32, len: u32, f: F) -> RS<Vec<u8>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_read_param(session_id, fd, len);
    let result = f(param_binary).await?;
    fs::deserialize_fs_read_result(&result)
}

/// Asynchronously invoke the host `fs write` operation.
pub async fn async_invoke_host_fs_write<F>(session_id: OID, fd: u32, data: &[u8], f: F) -> RS<u32>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_write_param(session_id, fd, data);
    let result = f(param_binary).await?;
    fs::deserialize_fs_write_result(&result)
}

/// Asynchronously invoke the host `fs pread` operation.
pub async fn async_invoke_host_fs_pread<F>(
    session_id: OID,
    fd: u32,
    offset: u64,
    len: u32,
    f: F,
) -> RS<Vec<u8>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_pread_param(session_id, fd, offset, len);
    let result = f(param_binary).await?;
    fs::deserialize_fs_pread_result(&result)
}

/// Asynchronously invoke the host `fs pwrite` operation.
pub async fn async_invoke_host_fs_pwrite<F>(
    session_id: OID,
    fd: u32,
    offset: u64,
    data: &[u8],
    f: F,
) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_pwrite_param(session_id, fd, offset, data);
    let result = f(param_binary).await?;
    fs::deserialize_fs_pwrite_result(&result)
}

/// Asynchronously invoke the host `fs lseek` operation.
pub async fn async_invoke_host_fs_lseek<F>(
    session_id: OID,
    fd: u32,
    offset: i64,
    whence: u32,
    f: F,
) -> RS<u64>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_lseek_param(session_id, fd, offset, whence);
    let result = f(param_binary).await?;
    fs::deserialize_fs_lseek_result(&result)
}

/// Asynchronously invoke the host `fs fstat` operation.
pub async fn async_invoke_host_fs_fstat<F>(session_id: OID, fd: u32, f: F) -> RS<FsStat>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_fstat_param(session_id, fd);
    let result = f(param_binary).await?;
    fs::deserialize_fs_fstat_result(&result)
}

/// Asynchronously invoke the host `fs stat` operation.
pub async fn async_invoke_host_fs_stat<F>(session_id: OID, oid: OID, path: &str, f: F) -> RS<FsStat>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_stat_param(session_id, oid, path);
    let result = f(param_binary).await?;
    fs::deserialize_fs_stat_result(&result)
}

/// Asynchronously invoke the host `fs fsync` operation.
pub async fn async_invoke_host_fs_fsync<F>(session_id: OID, fd: u32, f: F) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_fsync_param(session_id, fd);
    let result = f(param_binary).await?;
    fs::deserialize_fs_fsync_result(&result)
}

/// Asynchronously invoke the host `fs readdir` operation.
pub async fn async_invoke_host_fs_readdir<F>(
    session_id: OID,
    oid: OID,
    path: &str,
    f: F,
) -> RS<Vec<FsDirEntry>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = fs::serialize_fs_readdir_param(session_id, oid, path);
    let result = f(param_binary).await?;
    fs::deserialize_fs_readdir_result(&result)
}

/// Adapter that wraps a [`ResultBatch`] as a synchronous [`ResultSet`].
pub struct ResultSetWrapper {
    batch: SMutex<ResultBatch>,
}

impl ResultSetWrapper {
    /// Create a new instance from the provided batch.
    pub fn new(batch: ResultBatch) -> ResultSetWrapper {
        ResultSetWrapper {
            batch: SMutex::new(batch),
        }
    }
}

impl ResultSet for ResultSetWrapper {
    fn next(&self) -> RS<Option<TupleValue>> {
        let mut batch = self.batch.lock()?;
        let t = batch.mut_rows().pop();
        Ok(t)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use mudu_binding::codec::syscall_payload::MessageKind;
    use mudu_binding::system::{command_invoke, query_invoke};
    use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
    use mudu_contract::database::sql_stmt_text::SQLStmtText;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_contract::tuple::tuple_value::TupleValue;
    use mudu_type::data_value::DataValue;

    #[test]
    fn kv_get_roundtrip() {
        let encoded = serialize_session_get_param(7, b"k1");
        let (kind, _) = syscall_payload::decode_frame(&encoded).unwrap();
        assert_eq!(kind, MessageKind::Get);
        let (oid, key) = deserialize_session_get_param(&encoded).unwrap();
        assert_eq!(oid, 7);
        assert_eq!(key, b"k1");

        let encoded_result = serialize_get_result(Some(b"v1"));
        let decoded_result = deserialize_get_result(&encoded_result).unwrap();
        assert_eq!(decoded_result, Some(b"v1".to_vec()));

        let encoded_none = serialize_get_result(None);
        assert_eq!(deserialize_get_result(&encoded_none).unwrap(), None);
    }

    #[test]
    fn kv_range_roundtrip() {
        let encoded = serialize_session_range_param(3, b"a", b"z");
        let (kind, _) = syscall_payload::decode_frame(&encoded).unwrap();
        assert_eq!(kind, MessageKind::Range);
        let (oid, start, end) = deserialize_session_range_param(&encoded).unwrap();
        assert_eq!(oid, 3);
        assert_eq!((start, end), (b"a".to_vec(), b"z".to_vec()));

        let encoded_result = serialize_range_result(&[
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec()),
        ]);
        let decoded_result = deserialize_range_result(&encoded_result).unwrap();
        assert_eq!(
            decoded_result,
            vec![
                (b"a".to_vec(), b"1".to_vec()),
                (b"b".to_vec(), b"2".to_vec())
            ]
        );
    }

    #[test]
    fn open_and_open_argv_helpers_roundtrip() {
        let oid = invoke_host_open(|input| {
            let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
            assert_eq!(kind, MessageKind::Open);
            assert_eq!(deserialize_open_param(&input).unwrap().worker_oid(), 0);
            Ok(serialize_open_result(15))
        })
        .unwrap();
        assert_eq!(oid, 15);

        let argv = UniSessionOpenArgv::new(7);
        let oid = invoke_host_open_argv(&argv, |input| {
            let decoded = deserialize_open_param(&input).unwrap();
            assert_eq!(decoded.worker_oid(), 7);
            Ok(serialize_open_result(21))
        })
        .unwrap();
        assert_eq!(oid, 21);
    }

    #[test]
    fn session_put_and_close_helpers_roundtrip() {
        invoke_host_session_put(5, b"k", b"v", |input| {
            let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
            assert_eq!(kind, MessageKind::Put);
            let (oid, key, value) = deserialize_session_put_param(&input).unwrap();
            assert_eq!(oid, 5);
            assert_eq!((key, value), (b"k".to_vec(), b"v".to_vec()));
            Ok(serialize_put_result())
        })
        .unwrap();

        invoke_host_close(9, |input| {
            let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
            assert_eq!(kind, MessageKind::Close);
            assert_eq!(deserialize_close_param(&input).unwrap(), 9);
            Ok(serialize_close_result())
        })
        .unwrap();
    }

    #[test]
    fn command_and_query_helpers_decode_serialized_results() {
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let affected = invoke_host_command(3, &stmt, &(), |input| {
            let (oid, _, _) = command_invoke::deserialize_command_param(&input).unwrap();
            assert_eq!(oid, 3);
            Ok(command_invoke::serialize_command_result(Ok(5)))
        })
        .unwrap();
        assert_eq!(affected, 5);

        let records = invoke_host_query::<i32, _>(4, &stmt, &(), |input| {
            let (oid, _, _) = query_invoke::deserialize_query_param(&input).unwrap();
            assert_eq!(oid, 4);
            Ok(query_invoke::serialize_query_result(Ok((
                mudu_contract::database::result_batch::ResultBatch::from(
                    4,
                    vec![TupleValue::from(vec![DataValue::from_i32(8)])],
                    true,
                ),
                <i32 as TupleDatum>::tuple_desc_static(&["value".to_string()]),
            ))))
        })
        .unwrap();
        assert_eq!(records.next_record().unwrap(), Some(8));
    }

    #[test]
    fn async_host_helpers_roundtrip_sync_payload_shapes() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let stmt = SQLStmtText::new("SELECT 1".to_string());

            let oid = async_invoke_host_open(|input: Vec<u8>| async move {
                let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
                assert_eq!(kind, MessageKind::Open);
                Ok(serialize_open_result(31))
            })
            .await
            .unwrap();
            assert_eq!(oid, 31);

            let affected = async_invoke_host_batch(6, &stmt, &(), |input: Vec<u8>| async move {
                let (oid, _, _) = command_invoke::deserialize_command_param(&input).unwrap();
                assert_eq!(oid, 6);
                Ok(command_invoke::serialize_command_result(Ok(2)))
            })
            .await
            .unwrap();
            assert_eq!(affected, 2);

            let records =
                async_invoke_host_query::<i32, _>(8, &stmt, &(), |input: Vec<u8>| async move {
                    let (oid, _, _) = query_invoke::deserialize_query_param(&input).unwrap();
                    assert_eq!(oid, 8);
                    Ok(query_invoke::serialize_query_result(Ok((
                        mudu_contract::database::result_batch::ResultBatch::from(
                            8,
                            vec![TupleValue::from(vec![DataValue::from_i32(13)])],
                            true,
                        ),
                        <i32 as TupleDatum>::tuple_desc_static(&["value".to_string()]),
                    ))))
                })
                .await
                .unwrap();
            assert_eq!(records.next_record().unwrap(), Some(13));

            let got = async_invoke_host_session_get(9, b"k", |input: Vec<u8>| async move {
                let (oid, key) = deserialize_session_get_param(&input).unwrap();
                assert_eq!(oid, 9);
                assert_eq!(key, b"k");
                Ok(serialize_get_result(Some(b"v")))
            })
            .await
            .unwrap();
            assert_eq!(got, Some(b"v".to_vec()));
        })
        .unwrap();
    }
}
