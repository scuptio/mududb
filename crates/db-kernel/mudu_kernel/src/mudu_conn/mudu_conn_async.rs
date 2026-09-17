use async_trait::async_trait;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
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
use mudu_sys::common::provider_type::ProviderType;
use mudu_sys::contract::async_mode::AsyncMode;
use mudu_sys::contract::async_stream::AsyncStream;
use mudu_sys::sync::async_::{AMutex, AMutexGuard};
use mudu_sys::sync::SMutex;
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_type::StmtType;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use crate::contract::app_schema::current_schema;
use crate::mudu_conn::mudu_prepared_stmt::MuduPreparedStmt;
use crate::server::worker_local::{try_current_worker_local, WorkerExecute, WorkerLocalRef};
use crate::server::worker_snapshot::KvItem;
use crate::sql::describer::Describer;
use crate::x_engine::api::DeltaOp;
use crate::x_engine::DataBin;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::provider::create_io_provider;

static DEFAULT_REMOTE_ADDR: OnceLock<SMutex<Option<String>>> = OnceLock::new();
static DEFAULT_REMOTE_WORKER_ID: OnceLock<SMutex<Option<OID>>> = OnceLock::new();
static DEFAULT_REMOTE_ASYNC_RUNTIME: OnceLock<SMutex<Option<Arc<dyn AsyncIoProvider>>>> =
    OnceLock::new();

enum ConnBackend {
    WorkerLocal(WorkerLocalRef),
    Remote(Arc<RemoteWorkerConn>),
}

struct RemoteWorkerConn {
    addr: String,
    worker_id: Option<OID>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    app_name: Option<String>,
    session_id: SMutex<Option<OID>>,
    stream: AMutex<Option<RemoteProtocolClient>>,
}

struct RemoteProtocolClient {
    stream: Box<dyn AsyncStream>,
    next_request_id: u64,
}

pub struct MuduConnAsync {
    backend: ConnBackend,
    parser: Arc<SQLParser>,
    session_id: Arc<SMutex<Option<OID>>>,
    /// Application this connection belongs to (from the `app=` connection
    /// string option); its schema is the namespace SQL and relation calls
    /// resolve against. `None`/sentinel names map to the default schema (see
    /// `contract::app_schema::current_schema`).
    app_name: Option<String>,
}

pub fn set_default_remote_addr(addr: Option<String>) {
    let slot = DEFAULT_REMOTE_ADDR.get_or_init(|| SMutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = addr;
    }
}

pub fn set_default_remote_worker_id(worker_id: Option<OID>) {
    let slot = DEFAULT_REMOTE_WORKER_ID.get_or_init(|| SMutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = worker_id;
    }
}

pub fn set_default_remote_async_runtime(async_runtime: Option<Arc<dyn AsyncIoProvider>>) {
    let slot = DEFAULT_REMOTE_ASYNC_RUNTIME.get_or_init(|| SMutex::new(None));
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

fn default_remote_async_runtime() -> Option<Arc<dyn AsyncIoProvider>> {
    DEFAULT_REMOTE_ASYNC_RUNTIME
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|guard| guard.clone()))
}

impl MuduConnAsync {
    pub fn new() -> RS<Self> {
        Self::new_with_runtime(default_remote_async_runtime())
    }

    pub fn new_with_runtime(async_runtime: Option<Arc<dyn AsyncIoProvider>>) -> RS<Self> {
        Self::new_with_runtime_and_app(async_runtime, None)
    }

    /// Create a connection bound to an application schema; see
    /// [`Self::app_name`].
    pub fn new_with_runtime_and_app(
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
        app_name: Option<String>,
    ) -> RS<Self> {
        if let Some(worker_local) = try_current_worker_local() {
            return Self::new_with_worker_local_and_app(worker_local, app_name);
        }
        let addr = default_remote_addr().ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                "current worker local is not set and no default remote mududb addr is configured"
            )
        })?;
        let parser = Arc::new(SQLParser::new()?);
        let remote = Arc::new(RemoteWorkerConn {
            addr,
            worker_id: default_remote_worker_id(),
            async_runtime,
            app_name,
            session_id: SMutex::new(None),
            stream: AMutex::new(None),
        });
        Ok(Self {
            backend: ConnBackend::Remote(remote),
            parser,
            session_id: Arc::new(SMutex::new(None)),
            app_name: None,
        })
    }

    /// Create a connection bound to an explicit worker-local backend.
    ///
    /// Production connections resolve the backend through
    /// [`try_current_worker_local`]; this constructor serves tests and hosts
    /// that hold the worker-local reference directly.
    pub fn new_with_worker_local(worker_local: WorkerLocalRef) -> RS<Self> {
        Self::new_with_worker_local_and_app(worker_local, None)
    }

    /// Create a worker-local connection bound to an application schema.
    pub fn new_with_worker_local_and_app(
        worker_local: WorkerLocalRef,
        app_name: Option<String>,
    ) -> RS<Self> {
        Ok(Self {
            backend: ConnBackend::WorkerLocal(worker_local),
            parser: Arc::new(SQLParser::new()?),
            session_id: Arc::new(SMutex::new(None)),
            app_name,
        })
    }

    fn app_name(&self) -> Option<&str> {
        self.app_name.as_deref()
    }

    fn parse_one(&self, sql: &dyn SQLStmt) -> RS<StmtType> {
        let stmt_list = self.parser.parse(&sql.to_sql_string())?;
        let mut stmts = stmt_list.into_stmts();
        if stmts.len() != 1 {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "expected exactly one statement"
            ));
        }
        Ok(stmts.remove(0))
    }

    async fn ensure_session_id(&self) -> RS<OID> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let trace = mudu_utils::task_trace!();
                if let Some(session_id) = *self.session_id.lock()? {
                    trace.watch("mudu_conn.ensure_session_id.stage", "cached");
                    return Ok(session_id);
                }
                let session_id = worker_local.open_async().await?;
                let mut guard = self.session_id.lock()?;
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
                .lock()?
                .ok_or_else(|| mudu_error!(ErrorCode::EntityNotFound, "no active session")),
            ConnBackend::Remote(remote) => remote.active_session_id().await,
        }
    }

    /// Point-read one relation row by primary key inside this connection's
    /// session transaction (bypasses SQL parsing and result-set
    /// serialization). The bare table name resolves against the schema of
    /// `app_name` (the procedure invocation's application).
    pub async fn relation_get_async(
        &self,
        app_name: &str,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local
                    .relation_get(session_id, Some(app_name), table, key, select)
                    .await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "relation access is not supported without worker-local context"
            )),
        }
    }

    /// Read-modify-write one relation row by primary key inside this
    /// connection's session transaction. Returns the affected row count. The
    /// bare table name resolves against the schema of `app_name`.
    pub async fn relation_update_async(
        &self,
        app_name: &str,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
        deltas: Vec<(AttrIndex, DeltaOp, DataBin)>,
    ) -> RS<u64> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local
                    .relation_update(session_id, Some(app_name), table, key, values, deltas)
                    .await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "relation access is not supported without worker-local context"
            )),
        }
    }

    /// Insert one relation row inside this connection's session transaction.
    /// The bare table name resolves against the schema of `app_name`.
    pub async fn relation_insert_async(
        &self,
        app_name: &str,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<()> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local
                    .relation_insert(session_id, Some(app_name), table, key, values)
                    .await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "relation access is not supported without worker-local context"
            )),
        }
    }

    /// Read one key inside this connection's session.
    pub async fn get_async(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local.get_async(session_id, key).await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "kv access is not supported without worker-local context"
            )),
        }
    }

    /// Put one key-value pair inside this connection's session.
    pub async fn put_async(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local.put_async(session_id, key, value).await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "kv access is not supported without worker-local context"
            )),
        }
    }

    /// Delete one key inside this connection's session.
    pub async fn delete_async(&self, key: &[u8]) -> RS<()> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local.delete_async(session_id, key).await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "kv access is not supported without worker-local context"
            )),
        }
    }

    /// Scan the `[start_key, end_key)` key range inside this connection's
    /// session.
    pub async fn range_async(&self, start_key: &[u8], end_key: &[u8]) -> RS<Vec<KvItem>> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local
                    .range_async(session_id, start_key, end_key)
                    .await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "kv access is not supported without worker-local context"
            )),
        }
    }

    /// Close the worker session this connection opened, if any; a later
    /// operation opens a fresh session. Closing a connection that never
    /// opened a session is a no-op.
    pub async fn close_async(&self) -> RS<()> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let Some(session_id) = *self.session_id.lock()? else {
                    return Ok(());
                };
                worker_local.close_async(session_id).await?;
                *self.session_id.lock()? = None;
                Ok(())
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "session close is not supported without worker-local context"
            )),
        }
    }

    /// Open an fs object inside this connection's session transaction.
    pub async fn fs_open_async(&self, oid: OID, path: &str, flags: u32) -> RS<u32> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                let fs_service = worker_local.fs_service()?;
                fs_service.fs_open(session_id, oid, path, flags).await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "fs access is not supported without worker-local context"
            )),
        }
    }
}

impl RemoteWorkerConn {
    /// The app name sent on the wire; mirrors the previous hardcoded
    /// `"default"` when the connection carries no application.
    fn app_name(&self) -> &str {
        self.app_name.as_deref().unwrap_or("default")
    }

    async fn client(&self) -> RS<AMutexGuard<'_, Option<RemoteProtocolClient>>> {
        let mut guard = self.stream.lock().await;
        if guard.is_none() {
            *guard =
                Some(RemoteProtocolClient::connect(&self.addr, self.async_runtime.clone()).await?);
        }
        Ok(guard)
    }

    async fn ensure_session_id(&self) -> RS<OID> {
        if let Some(session_id) = *self.session_id.lock()? {
            return Ok(session_id);
        }
        let mut client_guard = self.client().await?;
        let client = client_guard
            .as_mut()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "remote worker client is missing"))?;
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
        let mut guard = self.session_id.lock()?;
        if let Some(existing) = *guard {
            return Ok(existing);
        }
        *guard = Some(session_id);
        Ok(session_id)
    }

    async fn active_session_id(&self) -> RS<OID> {
        self.session_id
            .lock()?
            .ok_or_else(|| mudu_error!(ErrorCode::EntityNotFound, "no active session"))
    }

    async fn batch_sql(&self, sql: String) -> RS<u64> {
        let _session_id = self.ensure_session_id().await?;
        let mut client_guard = self.client().await?;
        let client = client_guard
            .as_mut()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "remote worker client is missing"))?;
        let payload = encode_batch_request(
            client.take_request_id(),
            &ClientRequest::new(self.app_name(), sql),
        )?;
        let frame = client.send_and_receive(&payload).await?;
        Ok(decode_server_response(&frame)?.affected_rows())
    }

    async fn execute_sql(&self, sql: String) -> RS<u64> {
        let _session_id = self.ensure_session_id().await?;
        let mut client_guard = self.client().await?;
        let client = client_guard
            .as_mut()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "remote worker client is missing"))?;
        let payload = encode_client_request_with_message_type(
            MessageType::Execute,
            client.take_request_id(),
            &ClientRequest::new(self.app_name(), sql),
        )?;
        let frame = client.send_and_receive(&payload).await?;
        Ok(decode_server_response(&frame)?.affected_rows())
    }
}

impl RemoteProtocolClient {
    async fn connect(addr: &str, async_runtime: Option<Arc<dyn AsyncIoProvider>>) -> RS<Self> {
        let addr: SocketAddr = addr.parse().map_err(|e| {
            mudu_error!(
                ErrorCode::Parse,
                format!("parse remote mududb addr error: {addr}"),
                e
            )
        })?;
        let runtime = select_remote_runtime(async_runtime.or_else(default_remote_async_runtime));
        let stream = runtime.net().connect_tcp(addr).await?;
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
            .map_err(|e| mudu_error!(ErrorCode::Network, "write request frame error", e))?;

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
            return Err(mudu_error!(ErrorCode::Network, error.message()));
        }
        Ok(frame)
    }
}

fn select_remote_runtime(
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> Arc<dyn AsyncIoProvider> {
    if let Some(async_runtime) = async_runtime {
        #[cfg(target_os = "linux")]
        if async_runtime.mode() == AsyncMode::IoUring
            && !mudu_sys::io::worker_ring::has_current_worker_ring()
        {
            return create_io_provider(ProviderType::Tokio);
        }
        return async_runtime;
    }
    create_io_provider(ProviderType::Tokio)
}

async fn read_exact(stream: &mut dyn AsyncStream, buf: &mut [u8]) -> RS<()> {
    let mut done = 0usize;
    while done < buf.len() {
        let n = stream.read(&mut buf[done..]).await?;
        if n == 0 {
            return Err(mudu_error!(
                ErrorCode::Network,
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
                let current_schema = current_schema(self.app_name());
                let desc =
                    Describer::describe(worker_local.meta_mgr().as_ref(), &parsed, &current_schema)
                        .await?;
                Ok(Arc::new(MuduPreparedStmt::new(
                    worker_local.clone(),
                    self.session_id.clone(),
                    stmt,
                    Arc::new(desc),
                    self.app_name.clone(),
                )))
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "prepare is not supported without worker-local context"
            )),
        }
    }

    async fn exec_silent(&self, sql_text: String) -> RS<()> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                let _ = worker_local
                    .batch(
                        session_id,
                        self.app_name(),
                        Box::new(sql_text),
                        Box::new(()),
                    )
                    .await?;
                Ok(())
            }
            ConnBackend::Remote(remote) => {
                let _ = remote.batch_sql(sql_text).await?;
                Ok(())
            }
        }
    }

    async fn begin_tx(&self) -> RS<OID> {
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
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
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
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
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
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
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
                worker_local
                    .query(session_id, self.app_name(), sql, param)
                    .await
            }
            ConnBackend::Remote(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "query is not supported without worker-local context"
            )),
        }
    }

    async fn execute(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        match &self.backend {
            ConnBackend::WorkerLocal(worker_local) => {
                let session_id = self.ensure_session_id().await?;
                worker_local
                    .execute(session_id, self.app_name(), sql, param)
                    .await
            }
            ConnBackend::Remote(remote) => {
                if param.size() != 0 {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
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
                worker_local
                    .batch(session_id, self.app_name(), sql, param)
                    .await
            }
            ConnBackend::Remote(remote) => {
                if param.size() != 0 {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "batch with parameters is not supported without worker-local context"
                    ));
                }
                remote.batch_sql(sql.to_sql_string()).await
            }
        }
    }
}
