use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

pub async fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    mudu_adapter::syscall::mudu_query_async(oid, sql, params).await
}

pub async fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    mudu_adapter::syscall::mudu_command_async(oid, sql, params).await
}

pub async fn mudu_batch(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    mudu_adapter::syscall::mudu_batch_async(_oid, _sql, _params).await
}

pub async fn mudu_open() -> RS<OID> {
    mudu_adapter::syscall::mudu_open_async(0).await
}

pub async fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    mudu_adapter::syscall::mudu_open_argv_async(argv).await
}

pub async fn mudu_close(session_id: OID) -> RS<()> {
    mudu_adapter::syscall::mudu_close_async(session_id).await
}

pub async fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    mudu_adapter::syscall::mudu_get_async(session_id, key).await
}

pub async fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    mudu_adapter::syscall::mudu_put_async(session_id, key, value).await
}

pub async fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    mudu_adapter::syscall::mudu_range_async(session_id, start_key, end_key).await
}
