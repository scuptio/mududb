use crate::contract::meta_mgr::MetaMgr;
use crate::server::message_bus_api::MessageBusRef;
use crate::server::worker_snapshot::KvItem;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use std::cell::UnsafeCell;
use std::sync::Arc;

use crate::x_engine::api::XContract;

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
