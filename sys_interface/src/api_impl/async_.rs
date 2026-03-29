#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu::common::id::OID;
use mudu::common::result::RS;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::entity::Entity;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::entity_set::RecordSet;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::sql_params::SQLParams;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_binding::system::{command_invoke, query_invoke};
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql::Context;

use crate::host;

#[allow(dead_code)]
fn not_implemented<T>(name: &str) -> RS<T> {
    Err(mudu::m_error!(mudu::error::ec::EC::NotImplemented, name))
}

#[cfg(all(not(target_arch = "wasm32"), feature = "standalone-adapter"))]
pub use super::async_standalone::*;

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub use super::async_wasm::*;

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_query<R: Entity>(
    _oid: OID,
    _sql: &dyn SQLStmt,
    _params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    not_implemented("mudu_query")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_command(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    not_implemented("mudu_command")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_batch(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    not_implemented("mudu_batch")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_open() -> RS<OID> {
    not_implemented("mudu_open")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_open_argv(_argv: &UniSessionOpenArgv) -> RS<OID> {
    not_implemented("mudu_open_argv")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_close(_session_id: OID) -> RS<()> {
    not_implemented("mudu_close")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_get(_session_id: OID, _key: &[u8]) -> RS<Option<Vec<u8>>> {
    not_implemented("mudu_get")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_put(_session_id: OID, _key: &[u8], _value: &[u8]) -> RS<()> {
    not_implemented("mudu_put")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
pub async fn mudu_range(
    _session_id: OID,
    _start_key: &[u8],
    _end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    not_implemented("mudu_range")
}

pub async fn mudu_query_bytes(query_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = query_invoke::deserialize_query_param(query_in)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::m_error!(
            mudu::error::ec::EC::NoSuchElement,
            format!("no such session/context {}", oid)
        )
    })?;
    let response = context
        .query_raw_async(stmt, params)
        .await
        .and_then(|result_set| {
            let desc = result_set.desc().clone();
            Ok((result_set, desc))
        });
    let response = match response {
        Ok((result_set, desc)) => {
            let rows = super::drain_async_result_set(result_set).await?;
            Ok((ResultBatch::from(oid, rows, true), desc))
        }
        Err(err) => Err(err),
    };
    Ok(query_invoke::serialize_query_result(response))
}

pub async fn mudu_fetch_bytes(cursor: &[u8]) -> RS<Vec<u8>> {
    let oid = super::fetch_cursor_oid(cursor)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::m_error!(
            mudu::error::ec::EC::NoSuchElement,
            format!("no such session/context {}", oid)
        )
    })?;
    let response = super::drain_context_rows(&context).map(|rows| ResultBatch::from(oid, rows, true));
    super::serialize_fetch_result(response)
}

pub async fn mudu_command_bytes(command_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = command_invoke::deserialize_command_param(command_in)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::m_error!(
            mudu::error::ec::EC::NoSuchElement,
            format!("no such session/context {}", oid)
        )
    })?;
    Ok(command_invoke::serialize_command_result(
        context.command_async(stmt, params).await,
    ))
}

pub async fn mudu_batch_bytes(batch_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = command_invoke::deserialize_command_param(batch_in)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::m_error!(
            mudu::error::ec::EC::NoSuchElement,
            format!("no such session/context {}", oid)
        )
    })?;
    Ok(command_invoke::serialize_command_result(
        context.batch_async(stmt, params).await,
    ))
}

pub async fn mudu_open_bytes(open_in: &[u8]) -> RS<Vec<u8>> {
    let argv = host::deserialize_open_param(open_in)?;
    Ok(host::serialize_open_result(mudu_open_argv(&argv).await?))
}

pub async fn mudu_close_bytes(close_in: &[u8]) -> RS<Vec<u8>> {
    let session_id = host::deserialize_close_param(close_in)?;
    mudu_close(session_id).await?;
    Ok(host::serialize_close_result())
}

pub async fn mudu_get_bytes(get_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key) = host::deserialize_session_get_param(get_in)?;
    let value = mudu_get(session_id, &key).await?;
    Ok(host::serialize_get_result(value.as_deref()))
}

pub async fn mudu_put_bytes(put_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key, value) = host::deserialize_session_put_param(put_in)?;
    mudu_put(session_id, &key, &value).await?;
    Ok(host::serialize_put_result())
}

pub async fn mudu_range_bytes(range_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, start_key, end_key) = host::deserialize_session_range_param(range_in)?;
    let items = mudu_range(session_id, &start_key, &end_key).await?;
    Ok(host::serialize_range_result(&items))
}
