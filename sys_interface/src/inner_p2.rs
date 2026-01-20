use crate::host::{invoke_host_command, invoke_host_query};
use crate::inner_p2::mududb::api::system;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

wit_bindgen::generate!({
    path:"wit/sync",
    world:"api"
});

#[allow(unused)]
pub fn inner_query<R: Entity>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    invoke_host_query(
        xid,
        sql,
        params,
        |param| { Ok(system::query(&param)) },
    )
}

#[allow(unused)]
pub fn inner_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    invoke_host_command(
        xid,
        sql,
        params,
        |param| { Ok(system::command(&param)) },
    )
}
