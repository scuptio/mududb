//! Remote Mudud protocol backend implementation.

use crate::config;
use crate::result_set::LocalResultSet;
use crate::sql::replace_placeholders;
use crate::state;
use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::client::client::SyncClient;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, PutRequest, RangeScanRequest, SessionCloseRequest,
    SessionCreateRequest,
};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::SMutex;
use mudu_sys::sync::async_::mutex::AMutex;
use mudu_sys::sync::async_::rwlock::ARwLock;
use mudu_sys::task::sync::spawn_thread_named;
use mudu_utils::task_async::build_current_thread_runtime;
use scc::HashMap as SccHashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};

struct MududSession {
    client: SyncClient,
    remote_session_id: u128,
}

type SessionRef = Arc<SMutex<MududSession>>;

lazy_static! {
    static ref SESSIONS: SccHashMap<OID, SessionRef> = SccHashMap::new();
    static ref ASYNC_NATIVE_SESSIONS: ARwLock<HashMap<OID, Arc<AMutex<AsyncMududSession>>>> =
        ARwLock::new(HashMap::new());
}

struct AsyncMududSession {
    client: AsyncClientImpl,
    remote_session_id: u128,
}

type RangeResult = Vec<(Vec<u8>, Vec<u8>)>;

struct QueryRows {
    row_desc: TupleFieldDesc,
    rows: Vec<TupleValue>,
}

enum AsyncCommand {
    Open {
        session_id: OID,
        worker_id: OID,
        response: SyncSender<RS<()>>,
    },
    Close {
        session_id: OID,
        response: SyncSender<RS<()>>,
    },
    Get {
        session_id: OID,
        key: Vec<u8>,
        response: SyncSender<RS<Option<Vec<u8>>>>,
    },
    Put {
        session_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
        response: SyncSender<RS<()>>,
    },
    Range {
        session_id: OID,
        start_key: Vec<u8>,
        end_key: Vec<u8>,
        response: SyncSender<RS<RangeResult>>,
    },
    Query {
        session_id: OID,
        app_name: String,
        sql_text: String,
        response: SyncSender<RS<QueryRows>>,
    },
    Command {
        session_id: OID,
        app_name: String,
        sql_text: String,
        response: SyncSender<RS<u64>>,
    },
    Batch {
        session_id: OID,
        app_name: String,
        sql_text: String,
        response: SyncSender<RS<u64>>,
    },
}

struct AsyncManager {
    sender: Sender<AsyncCommand>,
}

static ASYNC_MANAGER: OnceLock<AsyncManager> = OnceLock::new();

fn async_manager() -> RS<&'static AsyncManager> {
    if let Some(manager) = ASYNC_MANAGER.get() {
        return Ok(manager);
    }
    let manager = AsyncManager::start()?;
    // If another thread initialized the manager concurrently, `set` returns Err
    // with our value; in either case a value is present afterwards.
    let _ = ASYNC_MANAGER.set(manager);
    ASYNC_MANAGER
        .get()
        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "async manager not initialized"))
}

/// Creates a new remote Mudud session.
pub fn mudu_open(argv: &UniSessionOpenArgv) -> RS<OID> {
    if config::mudud_async_session_loop() {
        return async_open(argv.worker_oid());
    }

    let addr: std::net::SocketAddr = config::mudud_addr()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud tcp address"))?
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

/// Asynchronous version of [`mudu_open`].
pub async fn mudu_open_async(argv: &UniSessionOpenArgv) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    let addr = config::mudud_addr()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud tcp address"))?;
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

/// Asynchronous version of [`mudu_close`].
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

/// Retrieves a value from a remote Mudud session.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    if config::mudud_async_session_loop() {
        return async_get(session_id, key);
    }

    with_session(session_id, |session| {
        session.client.get(session.remote_session_id, key.to_vec())
    })
}

/// Asynchronous version of [`mudu_get`].
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

/// Asynchronous version of [`mudu_put`].
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

/// Asynchronous version of [`mudu_range`].
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

/// Executes a query on a remote Mudud session and returns the resulting record set.
pub fn mudu_query<R: Entity>(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;

    if config::mudud_async_session_loop() {
        return async_query(session_id, app_name, sql_text);
    }

    with_session(session_id, |session| {
        let response = session.client.query(app_name.clone(), sql_text.clone())?;
        let desc = response.row_desc().clone();
        let rows = response.rows().to_vec();
        Ok(RecordSet::new(
            Arc::new(LocalResultSet::new(rows)),
            Arc::new(desc),
        ))
    })
}

/// Asynchronous version of [`mudu_query`].
pub async fn mudu_query_async<R: Entity>(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let response = session
        .client
        .query(ClientRequest::new(&app_name, &sql_text))
        .await?;
    let desc = response.row_desc().clone();
    let rows = response.rows().to_vec();
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(rows)),
        Arc::new(desc),
    ))
}

/// Executes a parameterized SQL command on a remote Mudud session.
pub fn mudu_command(session_id: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;

    if config::mudud_async_session_loop() {
        return async_command(session_id, app_name, sql_text);
    }

    with_session(session_id, |session| {
        let response = session.client.execute(app_name.clone(), sql_text.clone())?;
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

/// Asynchronous version of [`mudu_command`].
pub async fn mudu_command_async(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud app name"))?;
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let response = session
        .client
        .execute(ClientRequest::new(&app_name, &sql_text))
        .await?;
    Ok(response.affected_rows())
}

/// Asynchronous version of [`mudu_batch`].
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

fn with_session<R, F>(session_id: OID, f: F) -> RS<R>
where
    F: FnOnce(&mut MududSession) -> RS<R>,
{
    let entry = SESSIONS.get_sync(&session_id).ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("session {} does not exist", session_id)
        )
    })?;
    let session_ref = entry.get().clone();
    let mut session = session_ref
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "mudud session lock poisoned"))?;
    f(&mut session)
}

async fn async_session(session_id: OID) -> RS<Arc<AMutex<AsyncMududSession>>> {
    ASYNC_NATIVE_SESSIONS
        .read()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("session {} does not exist", session_id)
            )
        })
}

fn async_open(worker_id: OID) -> RS<OID> {
    let session_id = state::next_session_id();
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Open {
            session_id,
            worker_id,
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async open command error",
                e
            )
        })?;
    recv_response(rx)?;
    Ok(session_id)
}

fn async_close(session_id: OID) -> RS<()> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Close {
            session_id,
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async close command error",
                e
            )
        })?;
    recv_response(rx)
}

fn async_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Get {
            session_id,
            key: key.to_vec(),
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async get command error",
                e
            )
        })?;
    recv_response(rx)
}

fn async_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Put {
            session_id,
            key: key.to_vec(),
            value: value.to_vec(),
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async put command error",
                e
            )
        })?;
    recv_response(rx)
}

fn async_range(session_id: OID, start_key: &[u8], end_key: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Range {
            session_id,
            start_key: start_key.to_vec(),
            end_key: end_key.to_vec(),
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async range command error",
                e
            )
        })?;
    recv_response(rx)
}

fn async_query<R: Entity>(session_id: OID, app_name: String, sql_text: String) -> RS<RecordSet<R>> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Query {
            session_id,
            app_name,
            sql_text,
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async query command error",
                e
            )
        })?;
    let response = recv_response(rx)?;
    let desc = response.row_desc;
    let rows = response.rows;
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(rows)),
        Arc::new(desc),
    ))
}

fn async_command(session_id: OID, app_name: String, sql_text: String) -> RS<u64> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Command {
            session_id,
            app_name,
            sql_text,
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async command error",
                e
            )
        })?;
    recv_response(rx)
}

fn async_batch(session_id: OID, app_name: String, sql_text: String) -> RS<u64> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Batch {
            session_id,
            app_name,
            sql_text,
            response: tx,
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "send mudud async batch command error",
                e
            )
        })?;
    recv_response(rx)
}

fn recv_response<T>(rx: Receiver<RS<T>>) -> RS<T> {
    rx.recv()
        .map_err(|e| mudu_error!(ErrorCode::Thread, "receive mudud async response error", e))?
}

fn session_open_config_json(worker_id: OID) -> Option<String> {
    if worker_id == 0 {
        None
    } else {
        Some(
            serde_json::json!({
                "session_id": UniOid::from(0),
                "worker_id": UniOid::from(worker_id),
            })
            .to_string(),
        )
    }
}

impl AsyncManager {
    fn start() -> RS<Self> {
        let (sender, receiver) = mpsc::channel();
        let runtime = build_current_thread_runtime().map_err(|e| {
            mudu_error!(
                ErrorCode::Thread,
                "build mudud async manager runtime error",
                e
            )
        })?;
        spawn_thread_named("mudu-adapter-mudud-async", move || {
            runtime.block_on(async move {
                let mut sessions = HashMap::<OID, AsyncMududSession>::new();
                while let Ok(command) = receiver.recv() {
                    handle_async_command(&mut sessions, command).await;
                }
            });
        })
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Thread,
                "spawn mudud async manager thread error",
                e
            )
        })?;
        Ok(Self { sender })
    }
}

async fn handle_async_command(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    command: AsyncCommand,
) {
    match command {
        AsyncCommand::Open {
            session_id,
            worker_id,
            response,
        } => {
            let result = async {
                let addr = config::mudud_addr()
                    .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud tcp address"))?;
                let mut client = AsyncClientImpl::connect(addr.as_str()).await?;
                let remote_session_id = client
                    .create_session(SessionCreateRequest::new(session_open_config_json(
                        worker_id,
                    )))
                    .await?
                    .session_id();
                sessions.insert(
                    session_id,
                    AsyncMududSession {
                        client,
                        remote_session_id,
                    },
                );
                Ok(())
            }
            .await;
            let _ = response.send(result);
        }
        AsyncCommand::Close {
            session_id,
            response,
        } => {
            let result = async {
                let mut session = sessions.remove(&session_id).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                let _ = session
                    .client
                    .close_session(SessionCloseRequest::new(session.remote_session_id))
                    .await?;
                Ok(())
            }
            .await;
            let _ = response.send(result);
        }
        AsyncCommand::Get {
            session_id,
            key,
            response,
        } => {
            let result = async {
                let session = sessions.get_mut(&session_id).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                Ok(session
                    .client
                    .get(GetRequest::new(session.remote_session_id, key))
                    .await?
                    .into_value())
            }
            .await;
            let _ = response.send(result);
        }
        AsyncCommand::Put {
            session_id,
            key,
            value,
            response,
        } => {
            let result = async {
                let session = sessions.get_mut(&session_id).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                let put = session
                    .client
                    .put(PutRequest::new(session.remote_session_id, key, value))
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
            .await;
            let _ = response.send(result);
        }
        AsyncCommand::Range {
            session_id,
            start_key,
            end_key,
            response,
        } => {
            let result = async {
                let session = sessions.get_mut(&session_id).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                Ok(session
                    .client
                    .range_scan(RangeScanRequest::new(
                        session.remote_session_id,
                        start_key,
                        end_key,
                    ))
                    .await?
                    .into_items()
                    .into_iter()
                    .map(|kv| (kv.key().to_vec(), kv.value().to_vec()))
                    .collect())
            }
            .await;
            let _ = response.send(result);
        }
        AsyncCommand::Query {
            session_id,
            app_name,
            sql_text,
            response,
        } => {
            let result = async {
                let session = sessions.get_mut(&session_id).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                let response_data = session
                    .client
                    .query(ClientRequest::new(app_name, sql_text))
                    .await?;
                Ok(QueryRows {
                    row_desc: response_data.row_desc().clone(),
                    rows: response_data.rows().to_vec(),
                })
            }
            .await;
            let _ = response.send(result);
        }
        AsyncCommand::Command {
            session_id,
            app_name,
            sql_text,
            response,
        } => {
            let result = async {
                let session = sessions.get_mut(&session_id).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                let response_data = session
                    .client
                    .execute(ClientRequest::new(app_name, sql_text))
                    .await?;
                Ok(response_data.affected_rows())
            }
            .await;
            let _ = response.send(result);
        }
        AsyncCommand::Batch {
            session_id,
            app_name,
            sql_text,
            response,
        } => {
            let result = async {
                let session = sessions.get_mut(&session_id).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                let response_data = session
                    .client
                    .batch(ClientRequest::new(app_name, sql_text))
                    .await?;
                Ok(response_data.affected_rows())
            }
            .await;
            let _ = response.send(result);
        }
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    // Test-only helpers may use `panic!`, `todo!` or `unimplemented!` for
    // assertions and stubs; these are not production code paths.
    #![allow(clippy::panic, clippy::todo, clippy::unimplemented)]

    use super::*;
    use crate::config;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu_binding::universal::uni_oid::UniOid;
    use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
    use mudu_contract::database::sql_stmt_text::SQLStmtText;

    fn with_connection_env<T>(value: &str, f: impl FnOnce() -> RS<T>) -> RS<T> {
        let prev = mudu_sys::env_var::var("MUDU_CONNECTION");
        mudu_sys::env_var::set_var("MUDU_CONNECTION", value);
        let result = f();
        match prev {
            Some(prev) => mudu_sys::env_var::set_var("MUDU_CONNECTION", &prev),
            None => mudu_sys::env_var::remove_var("MUDU_CONNECTION"),
        }
        result
    }

    #[test]
    fn session_open_config_json_zero_worker_returns_none() {
        assert!(session_open_config_json(0).is_none());
    }

    #[test]
    fn session_open_config_json_nonzero_worker_contains_session_id_and_worker_id() -> RS<()> {
        let worker_id = 42;
        let json = match session_open_config_json(worker_id) {
            Some(json) => json,
            None => panic!("non-zero worker should yield JSON"),
        };

        let value: serde_json::Value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(err) => panic!("valid JSON: {err}"),
        };
        assert_eq!(
            value.get("session_id"),
            Some(&serde_json::json!(UniOid::from(0)))
        );
        assert_eq!(
            value.get("worker_id"),
            Some(&serde_json::json!(UniOid::from(worker_id)))
        );
        Ok(())
    }

    #[test]
    fn mudud_addr_returns_none_under_sqlite_config() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("sqlite://./mududb_test.db", || {
            assert_eq!(config::mudud_addr(), None);
            Ok(())
        })
    }

    #[test]
    fn mudud_app_name_returns_none_under_sqlite_config() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("sqlite://./mududb_test.db", || {
            assert_eq!(config::mudud_app_name(), None);
            Ok(())
        })
    }

    #[test]
    fn mudu_open_returns_database_error_when_mudud_addr_missing() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("sqlite://./mududb_test.db", || {
            let argv = UniSessionOpenArgv {
                worker_id: UniOid::from(1),
            };
            let err = match mudu_open(&argv) {
                Ok(_) => panic!("expected database error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::Database);
            assert!(err.to_string().contains("missing mudud tcp address"));
            Ok(())
        })
    }

    #[test]
    fn mudu_query_returns_database_error_when_mudud_app_name_missing() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("sqlite://./mududb_test.db", || {
            let stmt = SQLStmtText::new("SELECT 1".to_string());
            let err = match mudu_query::<String>(1, &stmt, &()) {
                Ok(_) => panic!("expected mudu_query to fail"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::Database);
            assert!(err.to_string().contains("missing mudud app name"));
            Ok(())
        })
    }

    #[test]
    fn recv_response_maps_dropped_sender_to_thread_error() -> RS<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        drop(tx);
        let err = match recv_response::<()>(rx) {
            Ok(_) => panic!("expected thread error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::Thread);
        Ok(())
    }

    #[test]
    fn async_manager_returns_ok_and_second_call_returns_same_instance() -> RS<()> {
        let first = match async_manager() {
            Ok(manager) => manager,
            Err(err) => panic!("async manager should initialize: {err}"),
        };
        let second = match async_manager() {
            Ok(manager) => manager,
            Err(err) => panic!("async manager should already exist: {err}"),
        };
        assert!(std::ptr::eq(first, second));
        Ok(())
    }

    #[test]
    fn mudu_open_returns_database_error_when_mudud_addr_invalid() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("mudud://not-a-socket-addr/test", || {
            let argv = UniSessionOpenArgv {
                worker_id: UniOid::from(1),
            };
            let err = match mudu_open(&argv) {
                Ok(_) => panic!("expected database error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::Database);
            assert!(err.to_string().contains("invalid mudud tcp address"));
            Ok(())
        })
    }

    #[test]
    fn mudu_close_get_put_range_return_entity_not_found_for_missing_session() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("mudud://127.0.0.1:9999/test", || {
            let session_id = 9999;

            let err = match mudu_close(session_id) {
                Ok(_) => panic!("expected entity not found error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(
                err.to_string()
                    .contains(&format!("session {} does not exist", session_id))
            );

            let err = match mudu_get(session_id, b"key") {
                Ok(_) => panic!("expected entity not found error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(
                err.to_string()
                    .contains(&format!("session {} does not exist", session_id))
            );

            let err = match mudu_put(session_id, b"key", b"value") {
                Ok(_) => panic!("expected entity not found error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(
                err.to_string()
                    .contains(&format!("session {} does not exist", session_id))
            );

            let err = match mudu_range(session_id, b"start", b"end") {
                Ok(_) => panic!("expected entity not found error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(
                err.to_string()
                    .contains(&format!("session {} does not exist", session_id))
            );
            Ok(())
        })
    }

    #[test]
    fn mudu_query_and_command_return_entity_not_found_for_missing_session() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("mudud://127.0.0.1:9999/test", || {
            let session_id = 9999;
            let stmt = SQLStmtText::new("SELECT 1".to_string());

            let err = match mudu_query::<i32>(session_id, &stmt, &()) {
                Ok(_) => panic!("expected mudu_query to fail"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(
                err.to_string()
                    .contains(&format!("session {} does not exist", session_id))
            );

            let err = match mudu_command(session_id, &stmt, &()) {
                Ok(_) => panic!("expected mudu_command to fail"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(
                err.to_string()
                    .contains(&format!("session {} does not exist", session_id))
            );
            Ok(())
        })
    }

    #[test]
    fn mudu_batch_returns_not_implemented_with_params() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("mudud://127.0.0.1:9999/test", || {
            let stmt = SQLStmtText::new("INSERT INTO t VALUES (?)".to_string());
            let err = match mudu_batch(9999, &stmt, &42i32) {
                Ok(_) => panic!("expected not implemented error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::NotImplemented);
            assert!(
                err.to_string()
                    .contains("batch syscall does not support SQL parameters")
            );
            Ok(())
        })
    }

    #[test]
    fn mudu_batch_returns_entity_not_found_for_missing_session_with_empty_params() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("mudud://127.0.0.1:9999/test", || {
            let session_id = 9999;
            let stmt = SQLStmtText::new("INSERT INTO t VALUES (1)".to_string());
            let err = match mudu_batch(session_id, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(
                err.to_string()
                    .contains(&format!("session {} does not exist", session_id))
            );
            Ok(())
        })
    }

    #[test]
    fn mudu_query_and_command_return_parse_error_on_placeholder_mismatch() -> RS<()> {
        let _guard = config::test_lock().lock()?;
        config::reset_db_path_override_for_test();
        with_connection_env("mudud://127.0.0.1:9999/test", || {
            let stmt = SQLStmtText::new("SELECT ?1, ?2".to_string());

            let err = match mudu_query::<i32>(9999, &stmt, &42i32) {
                Ok(_) => panic!("expected mudu_query to fail"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::Parse);
            assert!(
                err.to_string()
                    .contains("parameter and placeholder count mismatch")
            );

            let err = match mudu_command(9999, &stmt, &42i32) {
                Ok(_) => panic!("expected mudu_command to fail"),
                Err(err) => err,
            };
            assert_eq!(err.ec(), ErrorCode::Parse);
            assert!(
                err.to_string()
                    .contains("parameter and placeholder count mismatch")
            );
            Ok(())
        })
    }
}
