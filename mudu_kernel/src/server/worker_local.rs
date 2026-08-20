use crate::contract::meta_mgr::MetaMgr;
use crate::server::fs_service::FsService;
use crate::server::message_bus_api::MessageBusRef;
use crate::server::worker_snapshot::KvItem;
use async_trait::async_trait;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use std::cell::UnsafeCell;
use std::sync::Arc;

use crate::x_engine::api::{DeltaOp, XContract};
use crate::x_engine::DataBin;

thread_local! {
    static CURRENT_WORKER_LOCAL: UnsafeCell<Option<WorkerLocalRef>> =
        const { UnsafeCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerExecute {
    BeginTx,
    CommitTx,
    RollbackTx,
}

#[async_trait]
pub trait WorkerLocal: Send + Sync {
    fn x_contract(&self) -> Arc<dyn XContract>;
    fn meta_mgr(&self) -> Arc<dyn MetaMgr>;
    fn message_bus(&self) -> MessageBusRef;

    /// Return the fs object IO service of this worker.
    ///
    /// The default reports fs syscalls as unavailable; the session-bound
    /// worker runtime overrides this with the worker's service instance.
    fn fs_service(&self) -> RS<Arc<FsService>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "fs syscalls are not available on this worker"
        ))
    }

    /// Return the session this worker-local view is bound to, if any.
    ///
    /// The SyscallPayload v1 fs frames carry no session id outside `fs-open`:
    /// fd-based operations and `fs-stat`/`fs-readdir` resolve the session of
    /// the calling procedure through this accessor. The default reports no
    /// bound session; the session-bound worker runtime overrides it.
    fn current_session_id(&self) -> Option<OID> {
        None
    }

    async fn open_async(&self) -> RS<OID>;

    async fn open_argv_async(&self, worker_id: OID) -> RS<OID> {
        if worker_id == 0 {
            self.open_async().await
        } else {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::NotImplemented,
                format!("worker-local open on worker {} is not supported", worker_id)
            ))
        }
    }

    async fn close_async(&self, session_id: OID) -> RS<()>;

    async fn execute_async(&self, session_id: OID, instruction: WorkerExecute) -> RS<()>;

    async fn put_async(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()>;

    async fn delete_async(&self, session_id: OID, key: &[u8]) -> RS<()>;

    async fn get_async(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>>;

    async fn range_async(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>>;

    async fn query(
        &self,
        oid: OID,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>>;

    async fn execute(&self, oid: OID, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64>;

    async fn batch(&self, oid: OID, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64>;

    /// Point-read one relation row by primary key inside the session
    /// transaction, returning the projected columns in `select` order.
    ///
    /// The default reports the relation syscalls as unavailable; the
    /// session-bound worker runtime overrides this with a direct
    /// [`XContract::read_key`] call that bypasses SQL parsing and result-set
    /// serialization.
    async fn relation_get(
        &self,
        session_id: OID,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let _ = (session_id, table, key, select);
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "relation syscalls are not available on this worker"
        ))
    }

    /// Read-modify-write one relation row by primary key inside the session
    /// transaction; `deltas` are evaluated against the latest committed row
    /// under the statement lock. Returns the affected row count.
    async fn relation_update(
        &self,
        session_id: OID,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
        deltas: Vec<(AttrIndex, DeltaOp, DataBin)>,
    ) -> RS<u64> {
        let _ = (session_id, table, key, values, deltas);
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "relation syscalls are not available on this worker"
        ))
    }

    /// Insert one relation row inside the session transaction; a duplicate
    /// primary key fails with `EntityAlreadyExists`.
    async fn relation_insert(
        &self,
        session_id: OID,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<()> {
        let _ = (session_id, table, key, values);
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "relation syscalls are not available on this worker"
        ))
    }
}

pub type WorkerLocalRef = Arc<dyn WorkerLocal + Send + Sync>;

pub(crate) fn set_current_worker_local(worker_local: WorkerLocalRef) {
    CURRENT_WORKER_LOCAL.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = Some(worker_local);
        }
    });
}

pub(crate) fn unset_current_worker_local() {
    CURRENT_WORKER_LOCAL.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = None;
        }
    });
}

pub fn try_current_worker_local() -> Option<WorkerLocalRef> {
    CURRENT_WORKER_LOCAL.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        let worker_local = unsafe { &*slot.get() };
        worker_local.as_ref().cloned()
    })
}
