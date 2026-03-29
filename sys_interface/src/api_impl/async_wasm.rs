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
    crate::inner_component_async::inner_query(oid, sql, params).await
}

pub async fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_component_async::inner_command(oid, sql, params).await
}

pub async fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_component_async::inner_batch(oid, sql, params).await
}

pub async fn mudu_open() -> RS<OID> {
    crate::inner_component_async::inner_open().await
}

pub async fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    crate::inner_component_async::inner_open_argv(argv).await
}

pub async fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_component_async::inner_close(session_id).await
}

pub async fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_component_async::inner_get(session_id, key).await
}

pub async fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_component_async::inner_put(session_id, key, value).await
}

pub async fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_component_async::inner_range(session_id, start_key, end_key).await
}
