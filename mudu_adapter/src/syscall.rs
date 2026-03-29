use crate::backend;
use crate::config;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

pub fn set_db_path(path: impl Into<std::path::PathBuf>) {
    config::set_db_path(path);
}

pub fn mudu_open(worker_id: OID) -> RS<OID> {
    backend::mudu_open(worker_id)
}

pub async fn mudu_open_async(worker_id: OID) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_open_async(worker_id).await
}

pub fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    backend::mudu_open_argv(argv)
}

pub async fn mudu_open_argv_async(argv: &UniSessionOpenArgv) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_open_argv_async(argv).await
}

pub fn mudu_close(session_id: OID) -> RS<()> {
    backend::mudu_close(session_id)
}

pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_close_async(session_id).await
}

pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    backend::mudu_get(session_id, key)
}

pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_get_async(session_id, key).await
}

pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    backend::mudu_put(session_id, key, value)
}

pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_put_async(session_id, key, value).await
}

pub fn mudu_set(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    mudu_put(session_id, key, value)
}

pub async fn mudu_set_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    mudu_put_async(session_id, key, value).await
}

pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    backend::mudu_range(session_id, start_key, end_key)
}

pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_range_async(session_id, start_key, end_key).await
}

pub fn mudu_query<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    backend::mudu_query(oid, sql_stmt, params)
}

pub async fn mudu_query_async<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_query_async(oid, sql_stmt, params).await
}

pub fn mudu_command(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    backend::mudu_command(oid, sql_stmt, params)
}

pub fn mudu_batch(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    backend::mudu_batch(oid, sql_stmt, params)
}

pub async fn mudu_command_async(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_command_async(oid, sql_stmt, params).await
}

pub async fn mudu_batch_async(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_batch_async(oid, sql_stmt, params).await
}
