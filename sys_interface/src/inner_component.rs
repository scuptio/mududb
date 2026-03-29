use crate::host::{
    invoke_host_batch, invoke_host_close, invoke_host_command, invoke_host_open, invoke_host_query,
    invoke_host_session_get, invoke_host_session_put, invoke_host_session_range,
};
use crate::inner_component::mududb::api::system;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
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
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    invoke_host_query(oid, sql, params, |param| Ok(system::query(&param)))
}

#[allow(unused)]
pub fn inner_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    invoke_host_command(oid, sql, params, |param| Ok(system::command(&param)))
}

#[allow(unused)]
pub fn inner_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    invoke_host_batch(oid, sql, params, |param| Ok(system::batch(&param)))
}

#[allow(unused)]
pub fn inner_open() -> RS<OID> {
    invoke_host_open(|param| Ok(system::open(&param)))
}

#[allow(unused)]
pub fn inner_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    crate::host::invoke_host_open_argv(argv, |param| Ok(system::open(&param)))
}

#[allow(unused)]
pub fn inner_close(session_id: OID) -> RS<()> {
    invoke_host_close(session_id, |param| Ok(system::close(&param)))
}

#[allow(unused)]
pub fn inner_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    invoke_host_session_get(session_id, key, |param| Ok(system::get(&param)))
}

#[allow(unused)]
pub fn inner_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    invoke_host_session_put(session_id, key, value, |param| Ok(system::put(&param)))
}

#[allow(unused)]
pub fn inner_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    invoke_host_session_range(session_id, start_key, end_key, |param| {
        Ok(system::range(&param))
    })
}
