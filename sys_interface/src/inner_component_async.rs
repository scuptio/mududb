use crate::host::{
    async_invoke_host_batch, async_invoke_host_close, async_invoke_host_command, async_invoke_host_open,
    async_invoke_host_query, async_invoke_host_session_get, async_invoke_host_session_put,
    async_invoke_host_session_range,
};
use crate::inner_component_async::mududb::async_api::system;
use mudu::common::id::OID;
use mudu::common::result::RS;
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

#[allow(unused)]
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

#[allow(unused)]
pub async fn inner_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    async_invoke_host_command(oid, sql, params, async |param| {
        Ok(system::command(param).await)
    })
    .await
}

#[allow(unused)]
pub async fn inner_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    async_invoke_host_batch(oid, sql, params, async |param| Ok(system::batch(param).await)).await
}

#[allow(unused)]
pub async fn inner_open() -> RS<OID> {
    async_invoke_host_open(async |param| Ok(system::open(param).await)).await
}

#[allow(unused)]
pub async fn inner_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    crate::host::async_invoke_host_open_argv(argv, async |param| Ok(system::open(param).await))
        .await
}

#[allow(unused)]
pub async fn inner_close(session_id: OID) -> RS<()> {
    async_invoke_host_close(session_id, async |param| Ok(system::close(param).await)).await
}

#[allow(unused)]
pub async fn inner_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    async_invoke_host_session_get(session_id, key, async |param| Ok(system::get(param).await)).await
}

#[allow(unused)]
pub async fn inner_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    async_invoke_host_session_put(session_id, key, value, async |param| {
        Ok(system::put(param).await)
    })
    .await
}

#[allow(unused)]
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
