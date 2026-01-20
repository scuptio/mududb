use crate::host::{async_invoke_host_command, async_invoke_host_query};
use crate::inner_p2_async::mududb::async_api::system;
use mudu::common::result::RS;
use mudu::common::xid::XID;
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
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    async_invoke_host_query(
        xid,
        sql,
        params,
        async |param| { Ok(system::query(param).await) },
    ).await
}

#[allow(unused)]
pub async fn inner_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    async_invoke_host_command(
        xid,
        sql,
        params,
        async |param| { Ok(system::command(param).await) },
    ).await
}
