use super::async_loop::{
    async_batch, async_close, async_command, async_get, async_open, async_put, async_query,
    async_range,
};
use super::topology::resolve_worker_addr;
use super::{MududSession, SESSIONS, session_open_config_json, with_session};
use crate::config;
use crate::result_set::LocalResultSet;
use crate::sql::to_data_values;
use crate::state;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_client::client::client::SyncClient;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::EntitySet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_sys::sync::SMutex;
use std::sync::Arc;

/// Creates a new remote Mudud session.
///
/// When `argv` carries a non-zero worker id, the session is pinned to that
/// worker: the client connects to the worker's own listener resolved from
/// the server topology instead of the configured base address.
pub fn mudu_open(argv: &UniSessionOpenArgv) -> RS<OID> {
    if config::mudud_async_session_loop() {
        return async_open(argv.worker_oid());
    }

    let addr: std::net::SocketAddr = resolve_worker_addr(argv.worker_oid())?
        .parse()
        .map_err(|e| mudu_error!(ErrorCode::Database, "invalid mudud tcp address", e))?;
    let mut client = SyncClient::connect(addr)?;
    let remote_session_id = client.create_session(session_open_config_json(argv.worker_oid()))?;
    let session_id = state::next_session_id();
    let session = Arc::new(SMutex::new(MududSession {
        client,
        remote_session_id,
    }));
    let _ = SESSIONS.insert_sync(session_id, session);
    Ok(session_id)
}

/// Closes a remote Mudud session.
pub fn mudu_close(session_id: OID) -> RS<()> {
    if config::mudud_async_session_loop() {
        return async_close(session_id);
    }

    let entry = SESSIONS.remove_sync(&session_id).ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("session {} does not exist", session_id)
        )
    })?;
    let session_ref = entry.1;
    let mut session = session_ref
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "mudud session lock poisoned"))?;
    let remote_session_id = session.remote_session_id;
    let _ = session.client.close_session(remote_session_id)?;
    Ok(())
}

/// Retrieves a value from a remote Mudud session.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    if config::mudud_async_session_loop() {
        return async_get(session_id, key);
    }

    with_session(session_id, |session| {
        session.client.get(session.remote_session_id, key.to_vec())
    })
}

/// Stores a value in a remote Mudud session.
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    if config::mudud_async_session_loop() {
        return async_put(session_id, key, value);
    }

    with_session(session_id, |session| {
        session
            .client
            .put(session.remote_session_id, key.to_vec(), value.to_vec())
    })
}

/// Scans a range of keys in a remote Mudud session.
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    if config::mudud_async_session_loop() {
        return async_range(session_id, start_key, end_key);
    }

    with_session(session_id, |session| {
        let items = session.client.range_scan(
            session.remote_session_id,
            start_key.to_vec(),
            end_key.to_vec(),
        )?;
        Ok(items
            .into_iter()
            .map(|kv| (kv.key().to_vec(), kv.value().to_vec()))
            .collect())
    })
}

/// Executes a query on a remote Mudud session and returns the resulting record set.
pub fn mudu_query<R: Entity>(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<EntitySet<R>> {
    let _trace = mudu_utils::task_trace!();
    // Send the template text (with `?` placeholders) plus the parameters so
    // identical statements share one plan-cache entry on the server.
    let sql_text = sql_stmt.to_sql_string();
    let params = to_data_values(params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;

    if config::mudud_async_session_loop() {
        return async_query(session_id, app_name, sql_text, params);
    }

    with_session(session_id, |session| {
        let response = session.client.query_with_oid_params(
            session.remote_session_id,
            app_name.clone(),
            sql_text.clone(),
            params,
        )?;
        let desc = response.row_desc().clone();
        let rows = response.rows().to_vec();
        Ok(EntitySet::new(
            Arc::new(LocalResultSet::new(rows)),
            Arc::new(desc),
        ))
    })
}

/// Executes a parameterized SQL command on a remote Mudud session.
pub fn mudu_command(session_id: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let sql_text = sql_stmt.to_sql_string();
    let params = to_data_values(params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;

    if config::mudud_async_session_loop() {
        return async_command(session_id, app_name, sql_text, params);
    }

    with_session(session_id, |session| {
        let response = session.client.execute_with_oid_params(
            session.remote_session_id,
            app_name.clone(),
            sql_text.clone(),
            params,
        )?;
        Ok(response.affected_rows())
    })
}

/// Executes a batch SQL statement on a remote Mudud session.
pub fn mudu_batch(session_id: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    if params.size() != 0 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;
    let sql_text = sql_stmt.to_sql_string();

    if config::mudud_async_session_loop() {
        return async_batch(session_id, app_name, sql_text);
    }

    with_session(session_id, |session| {
        let response = session.client.batch(app_name.clone(), sql_text.clone())?;
        Ok(response.affected_rows())
    })
}
