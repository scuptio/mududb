use crate::server_ur::procedure_runtime::ProcInvokerPtr;
use crate::server_ur::routing::{
    route_worker, RoutingContext, RoutingMode, SessionOpenConfig, SessionOpenTransferAction,
};
use crate::server_ur::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
use crate::server_ur::worker_registry::{WorkerIdentity, WorkerRegistry};
use crate::storage::worker_kv_store::{KvItem, WorkerKvStore, WorkerSnapshot};
use crate::x_log::worker_kv_log::{WorkerKvLog, WorkerLogLayout};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::xid::new_xid;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{ProcedureInvokeRequest, ProcedureInvokeResponse};
use scc::HashMap as SccHashMap;
use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
/// Per-worker execution context used by the `client` backend.
///
/// The `IoUringWorker` name is also historical. The type is shared by both the
/// Linux native `io_uring` loop and the non-Linux fallback loop so upper
/// layers do not need target-specific worker abstractions.
pub struct IoUringWorker {
    worker_index: usize,
    worker_id: OID,
    partition_ids: Vec<OID>,
    worker_count: usize,
    routing_mode: RoutingMode,
    store: WorkerKvStore,
    log_layout: WorkerLogLayout,
    procedure_runtime: Option<ProcInvokerPtr>,
    sessions: Arc<WorkerSessions>,
    registry: Arc<WorkerRegistry>,
}

#[derive(Default)]
struct WorkerSessions {
    session_owner: SccHashMap<OID, u64>,
    connection_sessions: SccHashMap<u64, Arc<SccHashMap<OID, ()>>>,
    session_contexts: SccHashMap<OID, Arc<SessionContext>>,
}

struct SessionBoundWorkerLocal {
    worker: Arc<IoUringWorker>,
    current_session_id: OID,
}

#[derive(Default)]
pub(crate) struct SessionContext {
    tx_manager: UnsafeCell<Option<WorkerTxManager>>,
}

struct WorkerTxManager {
    snapshot: WorkerSnapshot,
    staged_puts: BTreeMap<Vec<u8>, Vec<u8>>,
    log_buffer: Vec<(Vec<u8>, Vec<u8>)>,
}

unsafe impl Send for SessionContext {}
unsafe impl Sync for SessionContext {}

impl IoUringWorker {
    pub fn new(
        identity: WorkerIdentity,
        worker_count: usize,
        routing_mode: RoutingMode,
        log_dir: String,
        log_chunk_size: u64,
        procedure_runtime: Option<ProcInvokerPtr>,
        registry: Arc<WorkerRegistry>,
    ) -> RS<Self> {
        let log_layout = WorkerLogLayout::new(log_dir, identity.worker_id, log_chunk_size)?;
        let log = WorkerKvLog::new(log_layout.clone())?;
        Ok(Self {
            worker_index: identity.worker_index,
            worker_id: identity.worker_id,
            partition_ids: identity.partition_ids,
            worker_count,
            routing_mode,
            store: WorkerKvStore::new(identity.worker_index, log),
            log_layout,
            procedure_runtime,
            sessions: Arc::new(WorkerSessions::default()),
            registry,
        })
    }

    pub fn route_connection(&self, conn_id: u64, remote_addr: SocketAddr) -> usize {
        let ctx = RoutingContext::new(conn_id, remote_addr, None);
        route_worker(&ctx, self.routing_mode, self.worker_count)
    }

    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        self.store.put(key, value)
    }

    pub fn get(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.store.get(key)
    }

    pub async fn invoke_procedure(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>> {
        let procedure_runtime = self
            .procedure_runtime
            .as_ref()
            .ok_or_else(|| m_error!(EC::NotImplemented, "procedure runtime is not configured"))?;
        procedure_runtime
            .invoke(
                session_id,
                procedure_name,
                procedure_parameters,
                worker_local,
            )
            .await
    }

    pub fn create_session(&self, conn_id: u64) -> RS<OID> {
        loop {
            let session_id = new_xid();
            if self
                .sessions
                .session_owner
                .insert_sync(session_id, conn_id)
                .is_err()
            {
                continue;
            }
            let session_context = Arc::new(SessionContext::default());
            if self
                .sessions
                .session_contexts
                .insert_sync(session_id, session_context)
                .is_err()
            {
                let _ = self.sessions.session_owner.remove_sync(&session_id);
                continue;
            }
            self.connection_sessions(conn_id)
                .insert_sync(session_id, ());
            return Ok(session_id);
        }
    }

    pub fn close_session(&self, conn_id: u64, session_id: OID) -> RS<bool> {
        match self
            .sessions
            .session_owner
            .get_sync(&session_id)
            .map(|entry| *entry.get())
        {
            Some(owner_conn_id) if owner_conn_id == conn_id => {
                let _ = self.sessions.session_owner.remove_sync(&session_id);
                let _ = self.sessions.session_contexts.remove_sync(&session_id);
                if let Some(conn_sessions) = self.sessions.connection_sessions.get_sync(&conn_id) {
                    let conn_sessions = conn_sessions.get().clone();
                    let _ = conn_sessions.remove_sync(&session_id);
                }
                Ok(true)
            }
            Some(_) => Err(m_error!(
                EC::TxErr,
                format!(
                    "session {} does not belong to connection {}",
                    session_id, conn_id
                )
            )),
            None => Ok(false),
        }
    }

    pub fn close_connection_sessions(&self, conn_id: u64) -> RS<()> {
        if let Some((_conn_id, session_ids)) =
            self.sessions.connection_sessions.remove_sync(&conn_id)
        {
            session_ids.iter_sync(|session_id, _| {
                let _ = self.sessions.session_owner.remove_sync(session_id);
                let _ = self.sessions.session_contexts.remove_sync(session_id);
                true
            });
        }
        Ok(())
    }

    fn conn_id_for_session(&self, session_id: OID) -> RS<u64> {
        self.sessions
            .session_owner
            .get_sync(&session_id)
            .map(|entry| *entry.get())
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("session {} does not exist", session_id)
                )
            })
    }

    pub fn open_session(&self, session_id: OID) -> RS<OID> {
        let conn_id = self.conn_id_for_session(session_id)?;
        self.create_session(conn_id)
    }

    pub fn close_session_by_id(&self, session_id: OID) -> RS<()> {
        let conn_id = self.conn_id_for_session(session_id)?;
        let closed = self.close_session(conn_id, session_id)?;
        if closed {
            Ok(())
        } else {
            Err(m_error!(
                EC::NoSuchElement,
                format!("session {} does not exist", session_id)
            ))
        }
    }

    fn connection_sessions(&self, conn_id: u64) -> Arc<SccHashMap<OID, ()>> {
        if let Some(existing) = self.sessions.connection_sessions.get_sync(&conn_id) {
            return existing.get().clone();
        }
        let created = Arc::new(SccHashMap::new());
        match self
            .sessions
            .connection_sessions
            .insert_sync(conn_id, created.clone())
        {
            Ok(_) => created,
            Err((_conn_id, created)) => {
                if let Some(existing) = self.sessions.connection_sessions.get_sync(&conn_id) {
                    existing.get().clone()
                } else {
                    created
                }
            }
        }
    }

    fn session_context(&self, session_id: OID) -> RS<Arc<SessionContext>> {
        self.sessions
            .session_contexts
            .get_sync(&session_id)
            .map(|entry| entry.get().clone())
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("session {} does not exist", session_id)
                )
            })
    }

    fn ensure_session_exists(&self, session_id: OID) -> RS<()> {
        let _ = self.conn_id_for_session(session_id)?;
        Ok(())
    }

    pub fn get_for_connection(
        &self,
        conn_id: u64,
        session_id: OID,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        <Self as WorkerLocal>::get(self, session_id, key)
    }

    pub fn put_for_connection(
        &self,
        conn_id: u64,
        session_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        <Self as WorkerLocal>::put(self, session_id, key, value)
    }

    pub fn range_for_connection(
        &self,
        conn_id: u64,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        <Self as WorkerLocal>::range(self, session_id, start_key, end_key)
    }

    fn execute_tx(&self, session_id: OID, instruction: WorkerExecute) -> RS<()> {
        let session = self.session_context(session_id)?;
        match instruction {
            WorkerExecute::BeginTx => {
                if session.tx_manager_ref().is_some() {
                    return Err(m_error!(
                        EC::ExistingSuchElement,
                        format!("session {} already has an active transaction", session_id)
                    ));
                }
                session.set_tx_manager(Some(WorkerTxManager::new(self.store.begin_tx()?)));
                Ok(())
            }
            WorkerExecute::CommitTx => {
                let tx_manager = session.take_tx_manager().ok_or_else(|| {
                    m_error!(
                        EC::NoSuchElement,
                        format!("session {} has no active transaction", session_id)
                    )
                })?;
                let snapshot = tx_manager.snapshot().clone();
                let xid = tx_manager.xid();
                self.store
                    .commit_put_batch(&snapshot, xid, tx_manager.into_log_buffer())?;
                Ok(())
            }
            WorkerExecute::RollbackTx => {
                let tx_manager = session.take_tx_manager().ok_or_else(|| {
                    m_error!(
                        EC::NoSuchElement,
                        format!("session {} has no active transaction", session_id)
                    )
                })?;
                self.store.rollback_tx(tx_manager.xid())?;
                Ok(())
            }
        }
    }

    fn put_in_session(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let session = self.session_context(session_id)?;
        match session.tx_manager_mut().as_mut() {
            Some(tx_manager) => {
                tx_manager.put(key, value);
                Ok(())
            }
            None => self.store.put(key, value),
        }
    }

    fn get_in_session(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        let session = self.session_context(session_id)?;
        let staged = session
            .tx_manager_ref()
            .as_ref()
            .and_then(|tx_manager| tx_manager.get(key));
        match staged {
            Some(value) => Ok(Some(value)),
            None => match session.tx_manager_ref().as_ref() {
                Some(tx_manager) => self.store.get_with_snapshot(tx_manager.snapshot(), key),
                None => self.store.get(key),
            },
        }
    }

    fn range_in_session(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        let session = self.session_context(session_id)?;
        let staged = session
            .tx_manager_ref()
            .as_ref()
            .map(|tx_manager| tx_manager.staged_items_in_range(start_key, end_key))
            .unwrap_or_default();

        let mut merged = BTreeMap::new();
        let base_items = match session.tx_manager_ref().as_ref() {
            Some(tx_manager) => {
                self.store
                    .range_scan_with_snapshot(tx_manager.snapshot(), start_key, end_key)?
            }
            None => self.store.range_scan(start_key, end_key)?,
        };
        for item in base_items {
            merged.insert(item.key, item.value);
        }
        for (key, value) in staged {
            merged.insert(key, value);
        }
        Ok(merged
            .into_iter()
            .map(|(key, value)| KvItem { key, value })
            .collect())
    }

    fn ensure_session_owned_by_connection(&self, conn_id: u64, session_id: OID) -> RS<()> {
        match self
            .sessions
            .session_owner
            .get_sync(&session_id)
            .map(|entry| *entry.get())
        {
            Some(owner_conn_id) if owner_conn_id == conn_id => Ok(()),
            Some(_) => Err(m_error!(
                EC::TxErr,
                format!(
                    "session {} does not belong to connection {}",
                    session_id, conn_id
                )
            )),
            None => Err(m_error!(
                EC::NoSuchElement,
                format!("session {} does not exist", session_id)
            )),
        }
    }

    pub async fn handle_procedure_request(
        &self,
        conn_id: u64,
        request: &ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse> {
        let session_id = request.session_id() as OID;
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        let worker_local: WorkerLocalRef = Arc::new(SessionBoundWorkerLocal {
            worker: Arc::new(self.clone()),
            current_session_id: session_id,
        });
        let result = self
            .invoke_procedure(
                session_id,
                request.procedure_name(),
                request.procedure_parameters_owned(),
                worker_local,
            )
            .await?;
        Ok(ProcedureInvokeResponse::new(result))
    }

    pub fn worker_index(&self) -> usize {
        self.worker_index
    }

    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    pub fn partition_ids(&self) -> &[OID] {
        &self.partition_ids
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn registry(&self) -> &Arc<WorkerRegistry> {
        &self.registry
    }

    pub fn log_layout(&self) -> WorkerLogLayout {
        self.log_layout.clone()
    }

    pub fn replay_log_entry(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        self.store.put_local(key, value)
    }

    pub fn open_session_with_config(&self, conn_id: u64, config: SessionOpenConfig) -> RS<OID> {
        if config.target_worker_index() != self.worker_index()
            || config.worker_id() != self.worker_id()
        {
            return Err(m_error!(
                EC::InternalErr,
                format!(
                    "session open landed on worker index {} worker id {}, expected worker index {} worker id {}",
                    self.worker_index(),
                    self.worker_id(),
                    config.target_worker_index(),
                    config.worker_id()
                )
            ));
        }
        if config.session_id() == 0 {
            self.create_session(conn_id)
        } else {
            self.ensure_session_owned_by_connection(conn_id, config.session_id())?;
            Ok(config.session_id())
        }
    }

    pub fn prepare_connection_transfer(
        &self,
        conn_id: u64,
        action: Option<SessionOpenTransferAction>,
    ) -> RS<Vec<OID>> {
        if self.connection_has_active_tx(conn_id)? {
            return Err(m_error!(
                EC::TxErr,
                format!(
                    "connection {} cannot be transferred while a session transaction is active",
                    conn_id
                )
            ));
        }
        if let Some(action) = action {
            let config = action.config();
            if config.session_id() != 0 {
                self.ensure_session_owned_by_connection(conn_id, config.session_id())?;
            }
        }
        self.detach_connection_sessions(conn_id)
    }

    pub fn adopt_connection_sessions(&self, conn_id: u64, session_ids: &[OID]) -> RS<()> {
        if session_ids.is_empty() {
            return Ok(());
        }
        let conn_sessions = self.connection_sessions(conn_id);
        for &session_id in session_ids {
            self.sessions
                .session_owner
                .insert_sync(session_id, conn_id)
                .map_err(|_| {
                    m_error!(
                        EC::ExistingSuchElement,
                        format!("session {} already exists on target worker", session_id)
                    )
                })?;
            if self
                .sessions
                .session_contexts
                .insert_sync(session_id, Arc::new(SessionContext::default()))
                .is_err()
            {
                let _ = self.sessions.session_owner.remove_sync(&session_id);
                return Err(m_error!(
                    EC::ExistingSuchElement,
                    format!(
                        "session {} context already exists on target worker",
                        session_id
                    )
                ));
            }
            conn_sessions.insert_sync(session_id, ());
        }
        Ok(())
    }

    fn connection_has_active_tx(&self, conn_id: u64) -> RS<bool> {
        let session_ids = self.connection_session_ids(conn_id);
        for session_id in session_ids {
            let session = self.session_context(session_id)?;
            if session.tx_manager_ref().is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn connection_session_ids(&self, conn_id: u64) -> Vec<OID> {
        let Some(conn_sessions) = self.sessions.connection_sessions.get_sync(&conn_id) else {
            return Vec::new();
        };
        let mut session_ids = Vec::new();
        conn_sessions.get().iter_sync(|session_id, _| {
            session_ids.push(*session_id);
            true
        });
        session_ids
    }

    fn detach_connection_sessions(&self, conn_id: u64) -> RS<Vec<OID>> {
        let Some((_conn_id, conn_sessions)) =
            self.sessions.connection_sessions.remove_sync(&conn_id)
        else {
            return Ok(Vec::new());
        };
        let mut session_ids = Vec::new();
        conn_sessions.iter_sync(|session_id, _| {
            session_ids.push(*session_id);
            true
        });
        for &session_id in &session_ids {
            let _ = self.sessions.session_owner.remove_sync(&session_id);
            let _ = self.sessions.session_contexts.remove_sync(&session_id);
        }
        Ok(session_ids)
    }
}

fn worker_log_oid(worker_id: usize) -> OID {
    worker_id as u128 + 1
}

impl WorkerLocal for IoUringWorker {
    fn open(&self) -> RS<OID> {
        Err(m_error!(
            EC::NotImplemented,
            "open requires a session-bound worker local context"
        ))
    }

    fn open_argv(&self, worker_id: OID) -> RS<OID> {
        if worker_id == 0 {
            self.open()
        } else {
            Err(m_error!(
                EC::NotImplemented,
                format!(
                    "open on worker {} requires a session-bound worker local context",
                    worker_id
                )
            ))
        }
    }

    fn close(&self, session_id: OID) -> RS<()> {
        self.close_session_by_id(session_id)
    }

    fn execute(&self, session_id: OID, instruction: WorkerExecute) -> RS<()> {
        self.ensure_session_exists(session_id)?;
        self.execute_tx(session_id, instruction)
    }

    fn put(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        self.put_in_session(session_id, key, value)
    }

    fn get(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.get_in_session(session_id, key)
    }

    fn range(&self, session_id: OID, start_key: &[u8], end_key: &[u8]) -> RS<Vec<KvItem>> {
        self.range_in_session(session_id, start_key, end_key)
    }
}

impl WorkerLocal for SessionBoundWorkerLocal {
    fn open(&self) -> RS<OID> {
        self.worker.open_session(self.current_session_id)
    }

    fn open_argv(&self, worker_id: OID) -> RS<OID> {
        if worker_id == 0 || worker_id == self.worker.worker_id() {
            self.open()
        } else {
            Err(m_error!(
                EC::NotImplemented,
                format!(
                    "worker-local open cannot move from worker {} to worker {}",
                    self.worker.worker_id(),
                    worker_id
                )
            ))
        }
    }

    fn close(&self, session_id: OID) -> RS<()> {
        self.worker.close_session_by_id(session_id)
    }

    fn execute(&self, session_id: OID, instruction: WorkerExecute) -> RS<()> {
        <IoUringWorker as WorkerLocal>::execute(self.worker.as_ref(), session_id, instruction)
    }

    fn put(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        <IoUringWorker as WorkerLocal>::put(self.worker.as_ref(), session_id, key, value)
    }

    fn get(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        <IoUringWorker as WorkerLocal>::get(self.worker.as_ref(), session_id, key)
    }

    fn range(&self, session_id: OID, start_key: &[u8], end_key: &[u8]) -> RS<Vec<KvItem>> {
        <IoUringWorker as WorkerLocal>::range(self.worker.as_ref(), session_id, start_key, end_key)
    }
}

impl WorkerTxManager {
    fn new(snapshot: WorkerSnapshot) -> Self {
        Self {
            snapshot,
            staged_puts: BTreeMap::new(),
            log_buffer: Vec::new(),
        }
    }

    fn xid(&self) -> u64 {
        self.snapshot.xid()
    }

    fn snapshot(&self) -> &WorkerSnapshot {
        &self.snapshot
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.staged_puts.insert(key.clone(), value.clone());
        self.log_buffer.push((key, value));
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.staged_puts.get(key).cloned()
    }

    fn staged_items_in_range(&self, start_key: &[u8], end_key: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.staged_puts
            .iter()
            .filter(|(key, _)| is_key_in_range(key, start_key, end_key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn into_log_buffer(self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.log_buffer
    }
}

impl SessionContext {
    fn tx_manager_ref(&self) -> &Option<WorkerTxManager> {
        unsafe { &*self.tx_manager.get() }
    }

    fn tx_manager_mut(&self) -> &mut Option<WorkerTxManager> {
        unsafe { &mut *self.tx_manager.get() }
    }

    fn set_tx_manager(&self, tx_manager: Option<WorkerTxManager>) {
        unsafe {
            *self.tx_manager.get() = tx_manager;
        }
    }

    fn take_tx_manager(&self) -> Option<WorkerTxManager> {
        self.tx_manager_mut().take()
    }
}

fn is_key_in_range(key: &[u8], start_key: &[u8], end_key: &[u8]) -> bool {
    key >= start_key && (end_key.is_empty() || key < end_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_ur::procedure_runtime::ProcInvoker;
    use crate::server_ur::worker_local::{WorkerExecute, WorkerLocal};
    use crate::server_ur::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
    use async_trait::async_trait;
    use futures::FutureExt;
    use mudu::common::id::gen_oid;
    use std::env::temp_dir;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingProcedureRuntime {
        calls: Mutex<Vec<(OID, String, Vec<u8>)>>,
    }

    #[async_trait]
    impl ProcInvoker for RecordingProcedureRuntime {
        async fn invoke(
            &self,
            session_id: OID,
            procedure_name: &str,
            procedure_parameters: Vec<u8>,
            _worker_local: WorkerLocalRef,
        ) -> RS<Vec<u8>> {
            self.calls.lock().unwrap().push((
                session_id,
                procedure_name.to_string(),
                procedure_parameters.clone(),
            ));
            Ok(procedure_parameters)
        }
    }

    fn test_registry(worker_count: usize) -> (String, Arc<WorkerRegistry>) {
        let dir = temp_dir()
            .join(format!("worker_test_{}", gen_oid()))
            .to_string_lossy()
            .into_owned();
        let registry = load_or_create_worker_registry(&dir, worker_count).unwrap();
        (dir, registry)
    }

    fn test_worker(
        worker_index: usize,
        worker_count: usize,
        log_dir: &str,
        registry: Arc<WorkerRegistry>,
        procedure_runtime: Option<ProcInvokerPtr>,
    ) -> IoUringWorker {
        let identity = registry.worker(worker_index).cloned().unwrap();
        IoUringWorker::new(
            identity,
            worker_count,
            RoutingMode::ConnectionId,
            log_dir.to_string(),
            4096,
            procedure_runtime,
            registry,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn worker_invokes_configured_procedure_runtime() {
        let runtime = Arc::new(RecordingProcedureRuntime::default());
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, registry, Some(runtime.clone()));

        let response = worker
            .handle_procedure_request(
                11,
                &ProcedureInvokeRequest::new(9, "app/mod/proc", b"payload".to_vec()),
            )
            .await
            .unwrap_err();
        assert!(response.to_string().contains("does not exist"));

        let session_id = worker.create_session(11).unwrap();
        let response = worker
            .handle_procedure_request(
                11,
                &ProcedureInvokeRequest::new(session_id, "app/mod/proc", b"payload".to_vec()),
            )
            .await
            .unwrap();
        assert_eq!(response.into_result(), b"payload".to_vec());

        let calls = runtime.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, session_id);
        assert_eq!(calls[0].1, "app/mod/proc");
        assert_eq!(calls[0].2, b"payload".to_vec());
    }

    #[test]
    fn worker_session_lifecycle_is_connection_scoped() {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, registry, None);

        let session_id = worker.create_session(7).unwrap();
        assert!(worker.close_session(7, session_id).unwrap());

        let session_id = worker.create_session(7).unwrap();
        let err = worker.close_session(8, session_id).unwrap_err();
        assert!(err.to_string().contains("does not belong to connection 8"));

        worker.close_connection_sessions(7).unwrap();
        let err = worker
            .handle_procedure_request(
                7,
                &ProcedureInvokeRequest::new(session_id, "app/mod/proc", b"payload".to_vec()),
            )
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn worker_implements_worker_local_interface() {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, registry, None);

        let session_id = worker.create_session(1).unwrap();
        let local = SessionBoundWorkerLocal {
            worker: Arc::new(worker.clone()),
            current_session_id: session_id,
        };
        let local: &dyn WorkerLocal = &local;
        let opened = local.open().unwrap();
        local.execute(opened, WorkerExecute::BeginTx).unwrap();
        local.put(opened, b"a".to_vec(), b"1".to_vec()).unwrap();
        local.put(opened, b"b".to_vec(), b"2".to_vec()).unwrap();

        assert_eq!(local.get(opened, b"a").unwrap(), Some(b"1".to_vec()));
        assert_eq!(local.range(opened, b"a", b"z").unwrap().len(), 2);
        local.execute(opened, WorkerExecute::CommitTx).unwrap();
        assert_eq!(worker.get(b"a").unwrap(), Some(b"1".to_vec()));
        local.close(opened).unwrap();
    }

    #[test]
    fn worker_rollback_discards_staged_writes() {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, registry, None);

        let session_id = worker.create_session(1).unwrap();
        let local = SessionBoundWorkerLocal {
            worker: Arc::new(worker.clone()),
            current_session_id: session_id,
        };
        let local: &dyn WorkerLocal = &local;

        local.execute(session_id, WorkerExecute::BeginTx).unwrap();
        local.put(session_id, b"a".to_vec(), b"1".to_vec()).unwrap();
        assert_eq!(local.get(session_id, b"a").unwrap(), Some(b"1".to_vec()));
        local
            .execute(session_id, WorkerExecute::RollbackTx)
            .unwrap();

        assert_eq!(local.get(session_id, b"a").unwrap(), None);
        assert_eq!(worker.get(b"a").unwrap(), None);
    }

    #[test]
    fn worker_can_transfer_connection_sessions_between_partitions() {
        let (log_dir, registry) = test_registry(2);
        let source = test_worker(0, 2, &log_dir, registry.clone(), None);
        let target = test_worker(1, 2, &log_dir, registry.clone(), None);

        let conn_id = 41;
        let session_a = source.create_session(conn_id).unwrap();
        let session_b = source.create_session(conn_id).unwrap();
        let target_identity = registry.worker(1).unwrap();
        let action = SessionOpenTransferAction::new(
            7,
            SessionOpenConfig::new(session_a, target_identity.worker_id, 1),
        );

        let transferred = source
            .prepare_connection_transfer(conn_id, Some(action))
            .unwrap();
        assert_eq!(transferred.len(), 2);
        assert!(source.get_for_connection(conn_id, session_a, b"k").is_err());

        target
            .adopt_connection_sessions(conn_id, &transferred)
            .unwrap();
        assert_eq!(
            target
                .open_session_with_config(conn_id, action.config())
                .unwrap(),
            session_a
        );
        target
            .put_for_connection(conn_id, session_b, b"k".to_vec(), b"v".to_vec())
            .unwrap();
        assert_eq!(
            target.get_for_connection(conn_id, session_b, b"k").unwrap(),
            Some(b"v".to_vec())
        );
    }

    #[test]
    fn worker_rejects_transfer_with_active_transaction() {
        let (log_dir, registry) = test_registry(2);
        let worker = test_worker(0, 2, &log_dir, registry.clone(), None);
        let conn_id = 51;
        let session_id = worker.create_session(conn_id).unwrap();
        worker
            .execute_tx(session_id, WorkerExecute::BeginTx)
            .unwrap();

        let err = worker
            .prepare_connection_transfer(
                conn_id,
                Some(SessionOpenTransferAction::new(
                    1,
                    SessionOpenConfig::new(session_id, registry.worker(1).unwrap().worker_id, 1),
                )),
            )
            .unwrap_err();
        assert!(err.to_string().contains("cannot be transferred"));
    }

    #[test]
    fn worker_snapshot_isolation_hides_later_commits_from_existing_tx() {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, registry, None);

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        worker
            .execute_tx(session_a, WorkerExecute::BeginTx)
            .unwrap();
        <IoUringWorker as WorkerLocal>::put(&worker, session_b, b"k".to_vec(), b"v1".to_vec())
            .unwrap();

        assert_eq!(
            <IoUringWorker as WorkerLocal>::get(&worker, session_a, b"k").unwrap(),
            None
        );
        assert_eq!(
            <IoUringWorker as WorkerLocal>::get(&worker, session_b, b"k").unwrap(),
            Some(b"v1".to_vec())
        );
    }

    #[test]
    fn worker_snapshot_isolation_range_stays_stable_for_existing_tx() {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, registry, None);

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        <IoUringWorker as WorkerLocal>::put(&worker, session_b, b"a".to_vec(), b"1".to_vec())
            .unwrap();
        worker
            .execute_tx(session_a, WorkerExecute::BeginTx)
            .unwrap();
        <IoUringWorker as WorkerLocal>::put(&worker, session_b, b"b".to_vec(), b"2".to_vec())
            .unwrap();

        let rows = <IoUringWorker as WorkerLocal>::range(&worker, session_a, b"a", b"z").unwrap();
        assert_eq!(
            rows,
            vec![KvItem {
                key: b"a".to_vec(),
                value: b"1".to_vec()
            }]
        );
    }

    #[test]
    fn worker_first_committer_wins_without_locks() {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, registry, None);

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        worker
            .execute_tx(session_a, WorkerExecute::BeginTx)
            .unwrap();
        worker
            .execute_tx(session_b, WorkerExecute::BeginTx)
            .unwrap();
        <IoUringWorker as WorkerLocal>::put(&worker, session_a, b"k".to_vec(), b"v1".to_vec())
            .unwrap();
        <IoUringWorker as WorkerLocal>::put(&worker, session_b, b"k".to_vec(), b"v2".to_vec())
            .unwrap();

        worker
            .execute_tx(session_a, WorkerExecute::CommitTx)
            .unwrap();
        let err = worker
            .execute_tx(session_b, WorkerExecute::CommitTx)
            .unwrap_err();

        assert!(err.to_string().contains("write-write conflict"));
        assert_eq!(worker.get(b"k").unwrap(), Some(b"v1".to_vec()));
    }
}
