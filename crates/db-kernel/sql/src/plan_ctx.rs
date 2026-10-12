use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::api::XContract;
use ::xengine::tx_mgr::TxMgr;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use std::sync::Arc;

/// Context handed to the planner, shared by every executor it produces.
#[derive(Clone)]
pub struct PlanCtx {
    /// Transaction manager the executors' reads and writes are staged into.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// Metadata manager the executors read the catalog through.
    pub meta_mgr: Arc<dyn MetaMgr>,
    /// Storage engine contract the executors access table data through.
    pub x_contract: Arc<dyn XContract>,
    /// Async I/O runtime used by file-reading and file-writing commands.
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
}
