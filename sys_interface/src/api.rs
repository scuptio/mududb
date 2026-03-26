use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_p1::inner_query(oid, sql, params)
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_component::inner_query(oid, sql, params)
}

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub async fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_component_async::inner_query(oid, sql, params).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_query<R: Entity>(
    _oid: OID,
    _sql: &dyn SQLStmt,
    _params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    Err(mudu::m_error!(
        mudu::error::ec::EC::NotImplemented,
        "mudu_query"
    ))
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_p1::inner_command(oid, sql, params)
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_component::inner_command(oid, sql, params)
}

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub async fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_component_async::inner_command(oid, sql, params).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_command(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    Err(mudu::m_error!(
        mudu::error::ec::EC::NotImplemented,
        "mudu_command"
    ))
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub fn mudu_open() -> RS<OID> {
    crate::inner_p1::inner_open()
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub fn mudu_open() -> RS<OID> {
    crate::inner_component::inner_open()
}

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub async fn mudu_open() -> RS<OID> {
    crate::inner_component_async::inner_open().await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_open() -> RS<OID> {
    Err(mudu::m_error!(
        mudu::error::ec::EC::NotImplemented,
        "mudu_open"
    ))
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_p1::inner_close(session_id)
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_component::inner_close(session_id)
}

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub async fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_component_async::inner_close(session_id).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_close(_session_id: OID) -> RS<()> {
    Err(mudu::m_error!(
        mudu::error::ec::EC::NotImplemented,
        "mudu_close"
    ))
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_p1::inner_get(session_id, key)
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_component::inner_get(session_id, key)
}

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub async fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_component_async::inner_get(session_id, key).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_get(_session_id: OID, _key: &[u8]) -> RS<Option<Vec<u8>>> {
    Err(mudu::m_error!(
        mudu::error::ec::EC::NotImplemented,
        "mudu_get"
    ))
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_p1::inner_put(session_id, key, value)
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_component::inner_put(session_id, key, value)
}

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub async fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_component_async::inner_put(session_id, key, value).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_put(_session_id: OID, _key: &[u8], _value: &[u8]) -> RS<()> {
    Err(mudu::m_error!(
        mudu::error::ec::EC::NotImplemented,
        "mudu_put"
    ))
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_p1::inner_range(session_id, start_key, end_key)
}

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_component::inner_range(session_id, start_key, end_key)
}

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub async fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_component_async::inner_range(session_id, start_key, end_key).await
}

#[cfg(target_arch = "x86_64")]
pub fn mudu_range(
    _session_id: OID,
    _start_key: &[u8],
    _end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    Err(mudu::m_error!(
        mudu::error::ec::EC::NotImplemented,
        "mudu_range"
    ))
}
