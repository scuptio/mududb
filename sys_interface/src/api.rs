use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

#[cfg(all(target_arch = "wasm32", feature = "wasip1", not(feature = "wasip2")))]
pub fn mudu_query<
    R: Entity
>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_p1::inner_query(xid, sql, params)
}

#[cfg(all(target_arch = "wasm32", feature = "wasip2", not(feature = "async")))]
pub fn mudu_query<
    R: Entity
>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_p2::inner_query(xid, sql, params)
}

#[cfg(all(target_arch = "wasm32", feature = "wasip2", feature = "async"))]
pub async fn mudu_query<
    R: Entity
>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_p2_async::inner_query(xid, sql, params).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_query<
    R: Entity
>(
    _xid: XID,
    _sql: &dyn SQLStmt,
    _params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    Err(mudu::m_error!(mudu::error::ec::EC::NotImplemented, "mudu_query"))
}


#[cfg(all(target_arch = "wasm32", feature = "wasip1", not(feature = "wasip2")))]
pub fn mudu_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    crate::inner_p1::inner_command(xid, sql, params)
}

#[cfg(all(target_arch = "wasm32", feature = "wasip2", not(feature = "async")))]
pub fn mudu_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    crate::inner_p2::inner_command(xid, sql, params)
}

#[cfg(all(target_arch = "wasm32", feature = "wasip2", feature = "async"))]
pub async fn mudu_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    crate::inner_p2_async::inner_command(xid, sql, params).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_command(
    _xid: XID,
    _sql: &dyn SQLStmt,
    _params: &dyn SQLParams,
) -> RS<u64> {
    Err(mudu::m_error!(mudu::error::ec::EC::NotImplemented, "mudu_command"))
}


