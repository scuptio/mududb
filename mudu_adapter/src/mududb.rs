use crate::config;
use crate::result_set::LocalResultSet;
use crate::sql::{datum_type_for_id, replace_placeholders};
use crate::state;
use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
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
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use scc::HashMap as SccHashMap;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::runtime::Builder;
use tokio::sync::{Mutex as AsyncMutex, RwLock};

struct MududSession {
    client: SyncClient,
    remote_session_id: u128,
}

type SessionRef = Arc<Mutex<MududSession>>;

lazy_static! {
    static ref SESSIONS: SccHashMap<OID, SessionRef> = SccHashMap::new();
    static ref ASYNC_NATIVE_SESSIONS: RwLock<HashMap<OID, Arc<AsyncMutex<AsyncMududSession>>>> =
        RwLock::new(HashMap::new());
}

struct AsyncMududSession {
    client: AsyncClientImpl,
    remote_session_id: u128,
}

struct QueryRows {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
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
        response: SyncSender<RS<Vec<(Vec<u8>, Vec<u8>)>>>,
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
}

struct AsyncManager {
    sender: Sender<AsyncCommand>,
}

lazy_static! {
    static ref ASYNC_MANAGER: AsyncManager = AsyncManager::start();
}

pub fn mudu_open(argv: &UniSessionOpenArgv) -> RS<OID> {
    if config::mudud_async_session_loop() {
        return async_open(argv.worker_oid());
    }

    let addr = config::mudud_addr()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mudud tcp address"))?;
    let mut client = SyncClient::connect(addr.as_str())?;
    let remote_session_id = client.create_session(session_open_config_json(argv.worker_oid()))?;
    let session_id = state::next_session_id();
    let session = Arc::new(Mutex::new(MududSession {
        client,
        remote_session_id,
    }));
    let _ = SESSIONS.insert_sync(session_id, session);
    Ok(session_id)
}

pub async fn mudu_open_async(argv: &UniSessionOpenArgv) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    let addr = config::mudud_addr()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mudud tcp address"))?;
    let mut client = AsyncClientImpl::connect(addr.as_str()).await?;
    let remote_session_id = client
        .create_session(SessionCreateRequest::new(session_open_config_json(
            argv.worker_oid(),
        )))
        .await?
        .session_id();
    let session_id = state::next_session_id();
    let session = Arc::new(AsyncMutex::new(AsyncMududSession {
        client,
        remote_session_id,
    }));
    ASYNC_NATIVE_SESSIONS
        .write()
        .await
        .insert(session_id, session);
    Ok(session_id)
}

pub fn mudu_close(session_id: OID) -> RS<()> {
    if config::mudud_async_session_loop() {
        return async_close(session_id);
    }

    let entry = SESSIONS.remove_sync(&session_id).ok_or_else(|| {
        m_error!(
            EC::NoSuchElement,
            format!("session {} does not exist", session_id)
        )
    })?;
    let session_ref = entry.1;
    let mut session = session_ref
        .lock()
        .map_err(|_| m_error!(EC::InternalErr, "mudud session lock poisoned"))?;
    let remote_session_id = session.remote_session_id;
    let _ = session.client.close_session(remote_session_id)?;
    Ok(())
}

pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let session = {
        let mut sessions = ASYNC_NATIVE_SESSIONS.write().await;
        sessions.remove(&session_id)
    }
    .ok_or_else(|| {
        m_error!(
            EC::NoSuchElement,
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

pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    if config::mudud_async_session_loop() {
        return async_get(session_id, key);
    }

    with_session(session_id, |session| {
        session.client.get(session.remote_session_id, key.to_vec())
    })
}

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
        Err(m_error!(
            EC::NetErr,
            "remote put operation returned failure"
        ))
    }
}

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

pub fn mudu_query<R: Entity>(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mudud app name"))?;

    if config::mudud_async_session_loop() {
        return async_query(session_id, app_name, sql_text);
    }

    with_session(session_id, |session| {
        let response = session.client.query(app_name.clone(), sql_text.clone())?;
        let desc = build_desc(response.columns());
        let rows = response
            .rows()
            .iter()
            .map(|row| row_to_tuple_value(row))
            .collect::<RS<Vec<_>>>()?;
        Ok(RecordSet::new(
            Arc::new(LocalResultSet::new(rows)),
            Arc::new(desc),
        ))
    })
}

pub async fn mudu_query_async<R: Entity>(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mudud app name"))?;
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let response = session
        .client
        .query(ClientRequest::new(&app_name, &sql_text))
        .await?;
    let desc = build_desc(response.columns());
    let rows = response
        .rows()
        .iter()
        .map(|row| row_to_tuple_value(row))
        .collect::<RS<Vec<_>>>()?;
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(rows)),
        Arc::new(desc),
    ))
}

pub fn mudu_command(session_id: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mudud app name"))?;

    if config::mudud_async_session_loop() {
        return async_command(session_id, app_name, sql_text);
    }

    with_session(session_id, |session| {
        let response = session.client.execute(app_name.clone(), sql_text.clone())?;
        Ok(response.affected_rows())
    })
}

pub async fn mudu_command_async(
    session_id: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let app_name = config::mudud_app_name()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mudud app name"))?;
    let session = async_session(session_id).await?;
    let mut session = session.lock().await;
    let response = session
        .client
        .execute(ClientRequest::new(&app_name, &sql_text))
        .await?;
    Ok(response.affected_rows())
}

fn with_session<R, F>(session_id: OID, f: F) -> RS<R>
where
    F: FnOnce(&mut MududSession) -> RS<R>,
{
    let entry = SESSIONS.get_sync(&session_id).ok_or_else(|| {
        m_error!(
            EC::NoSuchElement,
            format!("session {} does not exist", session_id)
        )
    })?;
    let session_ref = entry.get().clone();
    let mut session = session_ref
        .lock()
        .map_err(|_| m_error!(EC::InternalErr, "mudud session lock poisoned"))?;
    f(&mut session)
}

async fn async_session(session_id: OID) -> RS<Arc<AsyncMutex<AsyncMududSession>>> {
    ASYNC_NATIVE_SESSIONS
        .read()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("session {} does not exist", session_id)
            )
        })
}

fn build_desc(columns: &[String]) -> TupleFieldDesc {
    let fields = columns
        .iter()
        .map(|column| DatumDesc::new(column.clone(), datum_type_for_id(DatTypeID::String)))
        .collect();
    TupleFieldDesc::new(fields)
}

fn row_to_tuple_value(row: &[String]) -> RS<TupleValue> {
    Ok(TupleValue::from(
        row.iter()
            .cloned()
            .map(DatValue::from_string)
            .collect::<Vec<_>>(),
    ))
}

fn async_open(worker_id: OID) -> RS<OID> {
    let session_id = state::next_session_id();
    let (tx, rx) = mpsc::sync_channel(1);
    ASYNC_MANAGER
        .sender
        .send(AsyncCommand::Open {
            session_id,
            worker_id,
            response: tx,
        })
        .map_err(|e| m_error!(EC::ThreadErr, "send mudud async open command error", e))?;
    recv_response(rx)?;
    Ok(session_id)
}

fn async_close(session_id: OID) -> RS<()> {
    let (tx, rx) = mpsc::sync_channel(1);
    ASYNC_MANAGER
        .sender
        .send(AsyncCommand::Close {
            session_id,
            response: tx,
        })
        .map_err(|e| m_error!(EC::ThreadErr, "send mudud async close command error", e))?;
    recv_response(rx)
}

fn async_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let (tx, rx) = mpsc::sync_channel(1);
    ASYNC_MANAGER
        .sender
        .send(AsyncCommand::Get {
            session_id,
            key: key.to_vec(),
            response: tx,
        })
        .map_err(|e| m_error!(EC::ThreadErr, "send mudud async get command error", e))?;
    recv_response(rx)
}

fn async_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let (tx, rx) = mpsc::sync_channel(1);
    ASYNC_MANAGER
        .sender
        .send(AsyncCommand::Put {
            session_id,
            key: key.to_vec(),
            value: value.to_vec(),
            response: tx,
        })
        .map_err(|e| m_error!(EC::ThreadErr, "send mudud async put command error", e))?;
    recv_response(rx)
}

fn async_range(session_id: OID, start_key: &[u8], end_key: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let (tx, rx) = mpsc::sync_channel(1);
    ASYNC_MANAGER
        .sender
        .send(AsyncCommand::Range {
            session_id,
            start_key: start_key.to_vec(),
            end_key: end_key.to_vec(),
            response: tx,
        })
        .map_err(|e| m_error!(EC::ThreadErr, "send mudud async range command error", e))?;
    recv_response(rx)
}

fn async_query<R: Entity>(session_id: OID, app_name: String, sql_text: String) -> RS<RecordSet<R>> {
    let (tx, rx) = mpsc::sync_channel(1);
    ASYNC_MANAGER
        .sender
        .send(AsyncCommand::Query {
            session_id,
            app_name,
            sql_text,
            response: tx,
        })
        .map_err(|e| m_error!(EC::ThreadErr, "send mudud async query command error", e))?;
    let response = recv_response(rx)?;
    let desc = build_desc(&response.columns);
    let rows = response
        .rows
        .iter()
        .map(|row| row_to_tuple_value(row))
        .collect::<RS<Vec<_>>>()?;
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(rows)),
        Arc::new(desc),
    ))
}

fn async_command(session_id: OID, app_name: String, sql_text: String) -> RS<u64> {
    let (tx, rx) = mpsc::sync_channel(1);
    ASYNC_MANAGER
        .sender
        .send(AsyncCommand::Command {
            session_id,
            app_name,
            sql_text,
            response: tx,
        })
        .map_err(|e| m_error!(EC::ThreadErr, "send mudud async command error", e))?;
    recv_response(rx)
}

fn recv_response<T>(rx: Receiver<RS<T>>) -> RS<T> {
    rx.recv()
        .map_err(|e| m_error!(EC::ThreadErr, "receive mudud async response error", e))?
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
    fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("mudu-adapter-mudud-async".to_string())
            .spawn(move || run_async_manager(receiver))
            .expect("spawn mudud async manager thread");
        Self { sender }
    }
}

fn run_async_manager(receiver: Receiver<AsyncCommand>) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build mudud async manager runtime");
    runtime.block_on(async move {
        let mut sessions = HashMap::<OID, AsyncMududSession>::new();
        while let Ok(command) = receiver.recv() {
            handle_async_command(&mut sessions, command).await;
        }
    });
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
                    .ok_or_else(|| m_error!(EC::DBInternalError, "missing mudud tcp address"))?;
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
                    m_error!(
                        EC::NoSuchElement,
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
                    m_error!(
                        EC::NoSuchElement,
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
                    m_error!(
                        EC::NoSuchElement,
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
                    Err(m_error!(
                        EC::NetErr,
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
                    m_error!(
                        EC::NoSuchElement,
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
                    m_error!(
                        EC::NoSuchElement,
                        format!("session {} does not exist", session_id)
                    )
                })?;
                let response_data = session
                    .client
                    .query(ClientRequest::new(app_name, sql_text))
                    .await?;
                Ok(QueryRows {
                    columns: response_data.columns().to_vec(),
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
                    m_error!(
                        EC::NoSuchElement,
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
    }
}
