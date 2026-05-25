use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::db_conn::DBConnAsync;
use mudu_contract::database::prepared_stmt::PreparedStmt;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::protocol::{
    decode_error_response, decode_server_response, decode_session_create_response,
    encode_batch_request, encode_client_request_with_message_type, encode_session_create_request,
    ClientRequest, Frame, FrameHeader, MessageType, SessionCreateRequest, HEADER_LEN,
};
use mudu_utils::sync::a_mutex::{AMutex, AMutexGuard};
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_type::StmtType;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};

use crate::async_rt::contract::{AsyncRuntime, AsyncStream};
use crate::mudu_conn::mudu_prepared_stmt::MuduPreparedStmt;
use crate::server::worker_local::{try_current_worker_local, WorkerExecute, WorkerLocalRef};
use crate::sql::describer::Describer;

static DEFAULT_REMOTE_ADDR: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static DEFAULT_REMOTE_WORKER_ID: OnceLock<Mutex<Option<OID>>> = OnceLock::new();
static DEFAULT_REMOTE_ASYNC_RUNTIME: OnceLock<Mutex<Option<Arc<dyn AsyncRuntime>>>> =
    OnceLock::new();

enum ConnBackend {
    WorkerLocal(WorkerLocalRef),
    Remote(Arc<RemoteWorkerConn>),
}

struct RemoteWorkerConn {
    addr: String,
    worker_id: Option<OID>,
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
    session_id: Mutex<Option<OID>>,
    stream: AMutex<Option<RemoteProtocolClient>>,
}

struct RemoteProtocolClient {
    stream: Box<dyn AsyncStream>,
    next_request_id: u64,
}

pub struct MuduConnAsync {
    backend: ConnBackend,
    parser: Arc<SQLParser>,
    session_id: Arc<Mutex<Option<OID>>>,
}

pub fn set_default_remote_addr(addr: Option<String>) {
    let slot = DEFAULT_REMOTE_ADDR.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = addr;
    }
}

pub fn set_default_remote_worker_id(worker_id: Option<OID>) {
    let slot = DEFAULT_REMOTE_WORKER_ID.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = worker_id;
    }
}

pub fn set_default_remote_async_runtime(async_runtime: Option<Arc<dyn AsyncRuntime>>) {
    let slot = DEFAULT_REMOTE_ASYNC_RUNTIME.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = async_runtime;
    }
}

pub fn clear_default_remote_if_current(addr: &str, worker_id: Option<OID>) {
    let current_addr = default_remote_addr();
    let current_worker_id = default_remote_worker_id();
    if current_addr.as_deref() != Some(addr) || current_worker_id != worker_id {
        return;
    }
    set_default_remote_addr(None);
    set_default_remote_worker_id(None);
    set_default_remote_async_runtime(None);
}

fn default_remote_addr() -> Option<String> {
    DEFAULT_REMOTE_ADDR
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|guard| guard.clone()))
}

fn default_remote_worker_id() -> Option<OID> {
    DEFAULT_REMOTE_WORKER_ID
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|guard| *guard))
}

fn default_remote_async_runtime() -> Option<Arc<dyn AsyncRuntime>> {
    DEFAULT_REMOTE_ASYNC_RUNTIME
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|guard| guard.clone()))
}

impl MuduConnAsync {
    pub fn new() -> RS<Self> {
        Self::new_with_runtime(default_remote_async_runtime())
    }

    pub fn new_with_runtime(async_runtime: Option<Arc<dyn AsyncRuntime>>) -> RS<Self> {
        if let Some(worker_local) = try_current_worker_local() {
            return Ok(Self {
                backend: ConnBackend::WorkerLocal(worker_local),
                parser: Arc::new(SQLParser::new()),
                session_id: Arc::new(Mutex::new(None)),
            });
        }
        let addr = default_remote_addr().ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                "current worker local is not set and no default remote mududb addr is configured"
            )
        })?;
        let parser = Arc::new(SQLParser::new());
        let remote = Arc::new(RemoteWorkerConn {
            addr,
            worker_id: default_remote_worker_id(),
            async_runtime,
            session_id: Mutex::new(None),
            stream: AMutex::new(None),
        });
        Ok(Self {
            backend: ConnBackend::Remote(remote),
            parser,
            session_id: Arc::new(Mutex::new(None)),
        })
    }

    fn parse_one(&self, sql: &dyn SQLStmt) -> RS<StmtType> {
        let stmt_list = self.parser.parse(&sql.to_sql_string())?;
        let mut stmts = stmt_list.into_stmts();
        if stmts.len() != 1 {
            return Err(m_error!(EC::ParseErr, "expected exactly one statement"));
        }
        Ok(stmts.remove(0))
    }

    async fn ensure_session_id(&self) -> RS<OID> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let trace = mudu_utils::task_trace!();
                if let Some(session_id) = *self.session_id.lock().unwrap() {
                    trace.watch("mudu_conn.ensure_session_id.stage", "cached");
                    return Ok(session_id);
                }
                let session_id = worker_local.open_async().await?;
                let mut guard = self.session_id.lock().unwrap();
                if let Some(existing) = *guard {
                    return Ok(existing);
                }
                *guard = Some(session_id);
                trace.watch("mudu_conn.ensure_session_id.stage", "store_done");
                Ok(session_id)
            }
            ConnBackend::Remote(remote) => remote.ensure_session_id().await,
        }
    }

    async fn active_session_id(&self) -> RS<OID> {
        match &self.backend {
            ConnBackend::WorkerLocal(_) => self
                .session_id
                .lock()
                .unwrap()
                .ok_or_else(|| m_error!(EC::NoSuchElement, "no active session")),
            ConnBackend::Remote(remote) => remote.active_session_id().await,
        }
    }
}

impl RemoteWorkerConn {
    async fn client(&self) -> RS<AMutexGuard<'_, Option<RemoteProtocolClient>>> {
        let mut guard = self.stream.lock().await;
        if guard.is_none() {
            *guard =
                Some(RemoteProtocolClient::connect(&self.addr, self.async_runtime.clone()).await?);
        }
        Ok(guard)
    }

    async fn ensure_session_id(&self) -> RS<OID> {
        if let Some(session_id) = *self.session_id.lock().unwrap() {
            return Ok(session_id);
        }
        let mut client_guard = self.client().await?;
        let client = client_guard
            .as_mut()
            .ok_or_else(|| m_error!(EC::InternalErr, "remote worker client is missing"))?;
        let request_id = client.take_request_id();
        let config_json = self.worker_id.map(|worker_id| {
            serde_json::json!({
                "session_id": 0,
                "worker_id": worker_id.to_string()
            })
            .to_string()
        });
        let payload =
            encode_session_create_request(request_id, &SessionCreateRequest::new(config_json))?;
        let frame = client.send_and_receive(&payload).await?;
        let session_id = decode_session_create_response(&frame)?.session_id();
        let mut guard = self.session_id.lock().unwrap();
        if let Some(existing) = *guard {
            return Ok(existing);
        }
        *guard = Some(session_id);
        Ok(session_id)
    }

    async fn active_session_id(&self) -> RS<OID> {
        self.session_id
            .lock()
            .unwrap()
            .ok_or_else(|| m_error!(EC::NoSuchElement, "no active session"))
    }

    async fn batch_sql(&self, sql: String) -> RS<u64> {
        let _session_id = self.ensure_session_id().await?;
        let mut client_guard = self.client().await?;
        let client = client_guard
            .as_mut()
            .ok_or_else(|| m_error!(EC::InternalErr, "remote worker client is missing"))?;
        let payload = encode_batch_request(
            client.take_request_id(),
            &ClientRequest::new("default", sql),
        )?;
        let frame = client.send_and_receive(&payload).await?;
        Ok(decode_server_response(&frame)?.affected_rows())
    }

    async fn execute_sql(&self, sql: String) -> RS<u64> {
        let _session_id = self.ensure_session_id().await?;
        let mut client_guard = self.client().await?;
        let client = client_guard
            .as_mut()
            .ok_or_else(|| m_error!(EC::InternalErr, "remote worker client is missing"))?;
        let payload = encode_client_request_with_message_type(
            MessageType::Execute,
            client.take_request_id(),
            &ClientRequest::new("default", sql),
        )?;
        let frame = client.send_and_receive(&payload).await?;
        Ok(decode_server_response(&frame)?.affected_rows())
    }
}

impl RemoteProtocolClient {
    async fn connect(addr: &str, async_runtime: Option<Arc<dyn AsyncRuntime>>) -> RS<Self> {
        let addr: SocketAddr = addr.parse().map_err(|e| {
            m_error!(
                EC::ParseErr,
                format!("parse remote mududb addr error: {addr}"),
                e
            )
        })?;
        let stream = match async_runtime.or_else(default_remote_async_runtime) {
            Some(async_runtime) => async_runtime.net().connect_tcp(addr).await?,
            None => {
                let runtime = crate::async_rt::tokio::runtime::TokioRuntime::new();
                runtime.net().connect_tcp(addr).await?
            }
        };
        Ok(Self {
            stream,
            next_request_id: 1,
        })
    }

    fn take_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    async fn send_and_receive(&mut self, payload: &[u8]) -> RS<Frame> {
        self.stream
            .write_all(payload)
            .await
            .map_err(|e| m_error!(EC::NetErr, "write request frame error", e))?;

        let mut header = [0u8; HEADER_LEN];
        read_exact(self.stream.as_mut(), &mut header).await?;
        let payload_len = FrameHeader::decode_header_bytes(&header)?.payload_len() as usize;
        let mut frame_bytes = Vec::with_capacity(HEADER_LEN + payload_len);
        frame_bytes.extend_from_slice(&header);
        if payload_len > 0 {
            let mut body = vec![0u8; payload_len];
            read_exact(self.stream.as_mut(), &mut body).await?;
            frame_bytes.extend_from_slice(&body);
        }
        let frame = Frame::decode(&frame_bytes)?;
        if frame.header().message_type() == MessageType::Error {
            let error = decode_error_response(&frame)?;
            return Err(m_error!(EC::NetErr, error.message()));
        }
        Ok(frame)
    }
}

async fn read_exact(stream: &mut dyn AsyncStream, buf: &mut [u8]) -> RS<()> {
    let mut done = 0usize;
    while done < buf.len() {
        let n = stream.read(&mut buf[done..]).await?;
        if n == 0 {
            return Err(m_error!(
                EC::NetErr,
                "unexpected eof while reading remote response"
            ));
        }
        done += n;
    }
    Ok(())
}

#[async_trait]
impl DBConnAsync for MuduConnAsync {
    async fn prepare(&self, stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let parsed = self.parse_one(stmt.as_ref())?;
                let desc = Describer::describe(worker_local.meta_mgr().as_ref(), parsed).await?;
                Ok(Arc::new(MuduPreparedStmt::new(
                    worker_local.clone(),
                    self.session_id.clone(),
                    stmt,
                    Arc::new(desc),
                )))
            }
            ConnBackend::Remote(_) => Err(m_error!(
                EC::NotImplemented,
                "prepare is not supported without worker-local context"
            )),
        }
    }

    async fn exec_silent(&self, sql_text: String) -> RS<()> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                let _ = worker_local
                    .batch(session_id, Box::new(sql_text), Box::new(()))
                    .await?;
                Ok(())
            }
            ConnBackend::Remote(remote) => {
                let _ = remote.batch_sql(sql_text).await?;
                Ok(())
            }
        }
    }

    async fn begin_tx(&self) -> RS<XID> {
        let trace = mudu_utils::task_trace!();
        let session_id = self.ensure_session_id().await?;
        trace.watch("mudu_conn.begin_tx.stage", "ensure_session_id_done");
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                worker_local
                    .execute_async(session_id, WorkerExecute::BeginTx)
                    .await?;
                trace.watch("mudu_conn.begin_tx.stage", "execute_async_done");
                Ok(session_id)
            }
            ConnBackend::Remote(_) => Err(m_error!(
                EC::NotImplemented,
                "transaction control is not supported without worker-local context"
            )),
        }
    }

    async fn rollback_tx(&self) -> RS<()> {
        let session_id = self.active_session_id().await?;
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                worker_local
                    .execute_async(session_id, WorkerExecute::RollbackTx)
                    .await
            }
            ConnBackend::Remote(_) => Err(m_error!(
                EC::NotImplemented,
                "transaction control is not supported without worker-local context"
            )),
        }
    }

    async fn commit_tx(&self) -> RS<()> {
        let session_id = self.active_session_id().await?;
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                worker_local
                    .execute_async(session_id, WorkerExecute::CommitTx)
                    .await
            }
            ConnBackend::Remote(_) => Err(m_error!(
                EC::NotImplemented,
                "transaction control is not supported without worker-local context"
            )),
        }
    }

    async fn query(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local.query(session_id, sql, param).await
            }
            ConnBackend::Remote(_) => Err(m_error!(
                EC::NotImplemented,
                "query is not supported without worker-local context"
            )),
        }
    }

    async fn execute(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local.execute(session_id, sql, param).await
            }
            ConnBackend::Remote(remote) => {
                if param.size() != 0 {
                    return Err(m_error!(
                        EC::NotImplemented,
                        "execute with parameters is not supported without worker-local context"
                    ));
                }
                remote.execute_sql(sql.to_sql_string()).await
            }
        }
    }

    async fn batch(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local.batch(session_id, sql, param).await
            }
            ConnBackend::Remote(remote) => {
                if param.size() != 0 {
                    return Err(m_error!(
                        EC::NotImplemented,
                        "batch with parameters is not supported without worker-local context"
                    ));
                }
                remote.batch_sql(sql.to_sql_string()).await
            }
        }
    }
}
