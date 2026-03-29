use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

#[allow(dead_code)]
pub fn invoke_host_command<F>(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams, f: F) -> RS<u64>
where
    F: Fn(OID, &dyn SQLStmt, &dyn SQLParams) -> RS<u64>,
{
    f(oid, sql, params)
}

#[allow(dead_code)]
pub fn invoke_host_query<R: Entity, F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: Fn(OID, &dyn SQLStmt, &dyn SQLParams) -> RS<RecordSet<R>>,
{
    f(oid, sql, params)
}
