use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_p1::inner_query(oid, sql, params)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_component::inner_query(oid, sql, params)
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_p1::inner_command(oid, sql, params)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_component::inner_command(oid, sql, params)
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_p1::inner_batch(oid, sql, params)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_component::inner_batch(oid, sql, params)
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_open() -> RS<OID> {
    crate::inner_p1::inner_open()
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_open() -> RS<OID> {
    crate::inner_component::inner_open()
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    crate::inner_p1::inner_open_argv(argv)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    crate::inner_component::inner_open_argv(argv)
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_p1::inner_close(session_id)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_component::inner_close(session_id)
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_p1::inner_get(session_id, key)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_component::inner_get(session_id, key)
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_p1::inner_put(session_id, key, value)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_component::inner_put(session_id, key, value)
}

#[cfg(all(feature = "wasip1", not(feature = "component-model")))]
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_p1::inner_range(session_id, start_key, end_key)
}

#[cfg(all(feature = "component-model", not(feature = "async")))]
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_component::inner_range(session_id, start_key, end_key)
}
