use super::{AsyncMududSession, session_open_config_json};
use crate::config;
use crate::result_set::LocalResultSet;
use crate::state;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::EntitySet;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, PutRequest, RangeScanRequest, SessionCloseRequest,
    SessionCreateRequest,
};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::task::sync::spawn_thread_named;
use mudu_type::data_value::DataValue;
use mudu_utils::task_async::build_current_thread_runtime;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};

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
        params: Vec<DataValue>,
        response: SyncSender<RS<QueryRows>>,
    },
    Command {
        session_id: OID,
        app_name: String,
        sql_text: String,
        params: Vec<DataValue>,
        response: SyncSender<RS<u64>>,
    },
    Batch {
        session_id: OID,
        app_name: String,
        sql_text: String,
        response: SyncSender<RS<u64>>,
    },
}

pub(super) struct AsyncManager {
    sender: Sender<AsyncCommand>,
}

static ASYNC_MANAGER: OnceLock<AsyncManager> = OnceLock::new();

pub(super) fn async_manager() -> RS<&'static AsyncManager> {
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

pub(super) fn async_open(worker_id: OID) -> RS<OID> {
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

pub(super) fn async_close(session_id: OID) -> RS<()> {
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

pub(super) fn async_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
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

pub(super) fn async_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
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

pub(super) fn async_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
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

pub(super) fn async_query<R: Entity>(
    session_id: OID,
    app_name: String,
    sql_text: String,
    params: Vec<DataValue>,
) -> RS<EntitySet<R>> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Query {
            session_id,
            app_name,
            sql_text,
            params,
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
    Ok(EntitySet::new(
        Arc::new(LocalResultSet::new(rows)),
        Arc::new(desc),
    ))
}

pub(super) fn async_command(
    session_id: OID,
    app_name: String,
    sql_text: String,
    params: Vec<DataValue>,
) -> RS<u64> {
    let (tx, rx) = mpsc::sync_channel(1);
    async_manager()?
        .sender
        .send(AsyncCommand::Command {
            session_id,
            app_name,
            sql_text,
            params,
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

pub(super) fn async_batch(session_id: OID, app_name: String, sql_text: String) -> RS<u64> {
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

pub(super) fn recv_response<T>(rx: Receiver<RS<T>>) -> RS<T> {
    rx.recv()
        .map_err(|e| mudu_error!(ErrorCode::Thread, "receive mudud async response error", e))?
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
            let result = handle_open(sessions, session_id, worker_id).await;
            let _ = response.send(result);
        }
        AsyncCommand::Close {
            session_id,
            response,
        } => {
            let result = handle_close(sessions, session_id).await;
            let _ = response.send(result);
        }
        AsyncCommand::Get {
            session_id,
            key,
            response,
        } => {
            let result = handle_get(sessions, session_id, key).await;
            let _ = response.send(result);
        }
        AsyncCommand::Put {
            session_id,
            key,
            value,
            response,
        } => {
            let result = handle_put(sessions, session_id, key, value).await;
            let _ = response.send(result);
        }
        AsyncCommand::Range {
            session_id,
            start_key,
            end_key,
            response,
        } => {
            let result = handle_range(sessions, session_id, start_key, end_key).await;
            let _ = response.send(result);
        }
        AsyncCommand::Query {
            session_id,
            app_name,
            sql_text,
            params,
            response,
        } => {
            let result = handle_query(sessions, session_id, app_name, sql_text, params).await;
            let _ = response.send(result);
        }
        AsyncCommand::Command {
            session_id,
            app_name,
            sql_text,
            params,
            response,
        } => {
            let result = handle_command(sessions, session_id, app_name, sql_text, params).await;
            let _ = response.send(result);
        }
        AsyncCommand::Batch {
            session_id,
            app_name,
            sql_text,
            response,
        } => {
            let result = handle_batch(sessions, session_id, app_name, sql_text).await;
            let _ = response.send(result);
        }
    }
}

/// Borrows the session of `session_id`, or fails with `EntityNotFound`.
fn session_mut(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
) -> RS<&mut AsyncMududSession> {
    sessions.get_mut(&session_id).ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("session {} does not exist", session_id)
        )
    })
}

async fn handle_open(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
    worker_id: OID,
) -> RS<()> {
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

async fn handle_close(sessions: &mut HashMap<OID, AsyncMududSession>, session_id: OID) -> RS<()> {
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

async fn handle_get(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
    key: Vec<u8>,
) -> RS<Option<Vec<u8>>> {
    let session = session_mut(sessions, session_id)?;
    Ok(session
        .client
        .get(GetRequest::new(session.remote_session_id, key))
        .await?
        .into_value())
}

async fn handle_put(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
    key: Vec<u8>,
    value: Vec<u8>,
) -> RS<()> {
    let session = session_mut(sessions, session_id)?;
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

async fn handle_range(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
) -> RS<RangeResult> {
    let session = session_mut(sessions, session_id)?;
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

async fn handle_query(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
    app_name: String,
    sql_text: String,
    params: Vec<DataValue>,
) -> RS<QueryRows> {
    let session = session_mut(sessions, session_id)?;
    let response_data = session
        .client
        .query(
            ClientRequest::new_with_oid(session.remote_session_id, app_name, sql_text)
                .with_params(params),
        )
        .await?;
    Ok(QueryRows {
        row_desc: response_data.row_desc().clone(),
        rows: response_data.rows().to_vec(),
    })
}

async fn handle_command(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
    app_name: String,
    sql_text: String,
    params: Vec<DataValue>,
) -> RS<u64> {
    let session = session_mut(sessions, session_id)?;
    let response_data = session
        .client
        .execute(
            ClientRequest::new_with_oid(session.remote_session_id, app_name, sql_text)
                .with_params(params),
        )
        .await?;
    Ok(response_data.affected_rows())
}

async fn handle_batch(
    sessions: &mut HashMap<OID, AsyncMududSession>,
    session_id: OID,
    app_name: String,
    sql_text: String,
) -> RS<u64> {
    let session = session_mut(sessions, session_id)?;
    let response_data = session
        .client
        .batch(ClientRequest::new(app_name, sql_text))
        .await?;
    Ok(response_data.affected_rows())
}
