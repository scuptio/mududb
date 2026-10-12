use super::topology::resolve_worker_addr_async;
use super::{ASYNC_NATIVE_SESSIONS, AsyncMududSession, async_session, session_open_config_json};
use crate::config;
use crate::result_set::LocalResultSet;
use crate::sql::to_data_values;
use crate::state;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::EntitySet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, PutRequest, RangeScanRequest, SessionCloseRequest,
    SessionCreateRequest,
};
use mudu_sys::sync::async_::mutex::AMutex;
use std::sync::Arc;

/// Asynchronous version of [`mudu_open`](super::sync_api::mudu_open).
pub async fn mudu_open_async(argv: &UniSessionOpenArgv) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    let addr = resolve_worker_addr_async(argv.worker_oid()).await?;
    let mut client = AsyncClientImpl::connect(addr.as_str()).await?;
    let remote_session_id = client
        .create_session(SessionCreateRequest::new(session_open_config_json(
            argv.worker_oid(),
        )))
        .await?
        .session_id();
    let session_id = state::next_session_id();
    let session = Arc::new(AMutex::new(AsyncMududSession {
        client,
        remote_session_id,
    }));
    ASYNC_NATIVE_SESSIONS
        .write()
        .await
        .insert(session_id, session);
    Ok(session_id)
}

/// Asynchronous version of [`mudu_close`](super::sync_api::mudu_close).
pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let session = {
        let mut sessions = ASYNC_NATIVE_SESSIONS.write().await;
        sessions.remove(&session_id)
    }
    .ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("session {} does not exist", session_id)
        )
    })?;
    let mut session = session.lock().await;
    let remote_session_id = session.remote_session_id;
    let _ = session
        .client
        .close_session(SessionCloseRequest::new(remote_session_id))
        .await?;
    Ok(())
}

/// Asynchronous version of [`mudu_get`](super::sync_api::mudu_get).
pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let remote_session_id = session.remote_session_id;
    Ok(session
        .client
        .get(GetRequest::new(remote_session_id, key.to_vec()))
        .await?
        .into_value())
}

/// Asynchronous version of [`mudu_put`](super::sync_api::mudu_put).
pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let remote_session_id = session.remote_session_id;
    let put = session
        .client
        .put(PutRequest::new(
            remote_session_id,
            key.to_vec(),
            value.to_vec(),
        ))
        .await?;
    if put.ok() {
        Ok(())
    } else {
        Err(mudu_error!(
            ErrorCode::Network,
            "remote put operation returned failure"
        ))
    }
}

/// Asynchronous version of [`mudu_range`](super::sync_api::mudu_range).
pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let remote_session_id = session.remote_session_id;
    let items = session
        .client
        .range_scan(RangeScanRequest::new(
            remote_session_id,
            start_key.to_vec(),
            end_key.to_vec(),
        ))
        .await?;
    Ok(items
        .into_items()
        .into_iter()
        .map(|kv| (kv.key().to_vec(), kv.value().to_vec()))
        .collect())
}

/// Asynchronous version of [`mudu_query`](super::sync_api::mudu_query).
pub async fn mudu_query_async<R: Entity>(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<EntitySet<R>> {
    let sql_text = sql_stmt.to_sql_string();
    let params = to_data_values(params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let remote_session_id = session.remote_session_id;
    let response = session
        .client
        .query(
            ClientRequest::new_with_oid(remote_session_id, &app_name, &sql_text)
                .with_params(params),
        )
        .await?;
    let desc = response.row_desc().clone();
    let rows = response.rows().to_vec();
    Ok(EntitySet::new(
        Arc::new(LocalResultSet::new(rows)),
        Arc::new(desc),
    ))
}

/// Asynchronous version of [`mudu_command`](super::sync_api::mudu_command).
pub async fn mudu_command_async(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = sql_stmt.to_sql_string();
    let params = to_data_values(params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let remote_session_id = session.remote_session_id;
    let response = session
        .client
        .execute(
            ClientRequest::new_with_oid(remote_session_id, &app_name, &sql_text)
                .with_params(params),
        )
        .await?;
    Ok(response.affected_rows())
}

/// Asynchronous version of [`mudu_batch`](super::sync_api::mudu_batch).
pub async fn mudu_batch_async(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    if params.size() != 0 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let response = session
        .client
        .batch(ClientRequest::new(&app_name, sql_stmt.to_sql_string()))
        .await?;
    Ok(response.affected_rows())
}
