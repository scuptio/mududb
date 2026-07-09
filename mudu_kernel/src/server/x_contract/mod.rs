pub(crate) use async_trait::async_trait;
pub(crate) use mudu::common::buf::Buf;
pub(crate) use mudu::common::id::{AttrIndex, OID};
pub(crate) use mudu::common::result::RS;
pub(crate) use mudu::error::ErrorCode;
pub(crate) use mudu::mudu_error;
pub(crate) use mudu_contract::tuple::build_tuple::build_tuple;
pub(crate) use mudu_contract::tuple::nullable_tuple::{NullableValue, TupleBuilder};
pub(crate) use mudu_contract::tuple::tuple_binary::TupleBinary as TupleRaw;
pub(crate) use mudu_contract::tuple::update_tuple::update_tuple;
pub(crate) use mudu_sys::sync::SMutex;
pub(crate) use mudu_utils::{gen_oid, scoped_task_trace, task_trace};
pub(crate) use std::collections::{BTreeMap, BTreeSet};
pub(crate) use std::ops::Bound;
pub(crate) use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
pub(crate) use std::sync::Arc;
pub(crate) use std::time::Duration;
pub(crate) use tracing::{debug, trace};

pub(crate) use crate::contract::meta_mgr::MetaMgr;
pub(crate) use crate::contract::schema_table::SchemaTable;
pub(crate) use crate::contract::table_desc::TableDesc;
pub(crate) use crate::meta::meta_mgr_factory::MetaMgrFactory;
pub(crate) use crate::server::message_bus_api::{
    current_message_bus, DeliveryMode, Envelope, MessageKind, OutgoingMessage, RecvFilter,
    ServerInstanceId,
};
pub(crate) use crate::server::partition_router::{
    PartitionRouter, DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
};
pub(crate) use crate::server::partition_rpc::{
    PartitionRpcRequest, PartitionRpcResponse, RpcBound,
};
pub(crate) use crate::server::worker_snapshot::{KvItem, WorkerSnapshot, WorkerSnapshotMgr};
pub(crate) use crate::server::worker_storage::WorkerStorage;
pub(crate) use crate::server::worker_tx_manager::WorkerTxManager;
pub(crate) use crate::server::x_lock_mgr::XLockMgr;
pub(crate) use crate::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogLayout};
pub(crate) use crate::wal::xl_batch::{new_xl_batch_writer, XLBatch};
pub(crate) use crate::wal::xl_data_op::{XLDelete, XLInsert, XLWrite};
pub(crate) use crate::wal::xl_entry::{TxOp, XLEntry};
pub(crate) use crate::x_engine::api::{
    AlterTable, Filter, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData,
    TupleRow, VecDatum, VecSelTerm, XContract,
};
pub(crate) use crate::x_engine::tx_mgr::TxMgr;
pub(crate) use mudu_sys::contract::async_io_provider::AsyncIoProvider;

pub(crate) type DataBin = Buf;

pub(crate) const PARTITION_RPC_REQUEST_KIND: MessageKind = MessageKind::User(0x7101);
pub(crate) const PARTITION_RPC_RESPONSE_KIND: MessageKind = MessageKind::User(0x7102);

pub struct WorkerXContract {
    server_instance_id: ServerInstanceId,
    worker_id: OID,
    default_unpartitioned_worker_id: OID,
    meta_mgr: Arc<dyn MetaMgr>,
    storage: Arc<WorkerStorage>,
    partition_router: PartitionRouter,
    partition_rpc_registered: AtomicBool,
    log: SMutex<Option<ChunkedWorkerLogBackend>>,
    log_layout: WorkerLogLayout,
    active_sessions: Arc<AtomicUsize>,
    /// Optional runtime I/O provider used by the io_uring worker loop. When
    /// present, WAL initialization scans the tail with the default (tokio)
    /// provider but the backend performs steady-state I/O via this provider.
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    snapshot_mgr: WorkerSnapshotMgr,
    tx_lock: XLockMgr,
    // commit_gate: AsyncMutex<()>,
}

pub struct VecCursor {
    inner: SMutex<VecCursorInner>,
}

pub struct VecCursorInner {
    rows: Vec<TupleRow>,
    index: usize,
}

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only contract.
pub type IoUringXContract = WorkerXContract;

pub(crate) mod cursor;
pub(crate) mod kv;
pub(crate) mod lifecycle;
pub(crate) mod ops;
pub(crate) mod params;
pub(crate) mod rpc;
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub(crate) mod tests;
pub(crate) mod trait_impl;
pub(crate) mod utils;

pub use params::{WorkerXContractParams, WorkerXContractWorkerLogParams};
