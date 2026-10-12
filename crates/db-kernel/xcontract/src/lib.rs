//! Kernel contract prelude: the worker-side execution contract and every
//! shared type the kernel crates consume.
//!
//! [`WorkerXContract`] is the per-worker implementation of the `XContract`
//! storage engine contract: transactional relation DML and worker-local KV,
//! WAL-backed commit, and cross-partition routing over the message bus. The
//! crate re-exports the contract traits and their parameter/result types from
//! the lower kernel layers so kernel crates depend on this single prelude.

#![warn(missing_docs)]

pub use async_trait::async_trait;
pub use mudu::common::buf::Buf;
pub use mudu::common::id::{AttrIndex, OID};
pub use mudu::common::result::RS;
pub use mudu::error::ErrorCode;
pub use mudu::mudu_error;
pub use mudu_sys::sync::SMutex;
pub use mudu_utils::{gen_oid, scoped_task_trace, task_trace};
pub use std::collections::{BTreeMap, BTreeSet};
pub use std::ops::Bound;
pub use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
pub use std::sync::Arc;
pub use std::time::Duration;
pub use tracing::{debug, trace};

pub use crate::partition_rpc::{PartitionRpcRequest, PartitionRpcResponse, RpcBound};
pub use crate::worker_storage::WorkerStorage;
pub use crate::worker_tx_manager::WorkerTxManager;
pub use crate::x_lock_mgr::XLockMgr;
pub use ::catalog::contract::message_bus_api::{
    current_message_bus, DeliveryMode, Envelope, MessageKind, OutgoingMessage, RecvFilter,
    ServerInstanceId,
};
pub use ::catalog::contract::meta_mgr::MetaMgr;
pub use ::catalog::contract::schema_table::SchemaTable;
pub use ::catalog::contract::table_desc::TableDesc;
pub use ::catalog::contract::worker_snapshot::{KvItem, WorkerSnapshot, WorkerSnapshotMgr};
pub use ::catalog::repl_ctrl::{RaftShareView, ReplAckWait, ReplControl};
pub use ::meta::meta_mgr_factory::MetaMgrFactory;
pub use ::storage::prepared_commit::CommitSequencer;
pub use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogLayout};
pub use ::wal::xl_batch::{new_xl_batch_writer, XLBatch};
pub use ::wal::xl_data_op::{XLDelete, XLInsert, XLWrite};
pub use ::wal::xl_entry::{TxOp, XLEntry};
#[cfg(test)]
pub use ::xengine::api::DeltaOp;
pub use ::xengine::api::{
    AlterTable, DeltaAssign, Filter, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor,
    RangeData, TupleRow, VecDatum, VecSelTerm, XContract,
};
pub use ::xengine::partition_router::{PartitionRouter, DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID};
pub use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
pub use mudu_sys::contract::async_io_provider::AsyncIoProvider;

/// Binary-encoded datum: the raw bytes of an encoded key or value field.
pub type DataBin = Buf;

/// Message-bus kind id of a [`PartitionRpcRequest`] payload.
pub const PARTITION_RPC_REQUEST_KIND: MessageKind = MessageKind::User(0x7101);
/// Message-bus kind id of a [`PartitionRpcResponse`] payload.
pub const PARTITION_RPC_RESPONSE_KIND: MessageKind = MessageKind::User(0x7102);

/// How a local commit must be routed in the current replication state.
pub enum ReplCommitRoute {
    /// Single-node (no replication) or the replication-mode primary with
    /// `sync = false`: apply inline exactly as before.
    Direct,
    /// Raft leader: stage through the worker's commit sequencer.
    Sequencer(Arc<dyn CommitSequencer>),
    /// Replication-mode primary with `sync = true`: apply inline, then wait
    /// for a replica's durable ack before answering the commit.
    DirectSync(Arc<dyn ReplAckWait>, Duration),
    /// Raft follower (or a leader still healing) or a replication-mode
    /// replica: reject the commit; the client must redirect to the leader.
    Reject,
}

/// Error for commits rejected because this node is not the serving raft
/// leader / replication primary (client redirects to the leader).
pub fn not_serving_leader_error() -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::DomainViolation,
        "not the serving leader: write requests must be sent to the serving raft leader or the configured primary"
    )
}

/// Per-worker implementation of the `XContract` storage engine contract:
/// transactional relation DML and worker-local KV, WAL-backed commit, and
/// cross-partition routing over the message bus.
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
    /// Raft state shared with this worker when the server runs a
    /// replication cluster; installed after construction by the server.
    raft_share: SMutex<Option<Arc<dyn RaftShareView>>>,
    /// The worker's commit sequencer (leader commit path), installed with
    /// `raft_share` when the worker's replication tasks start.
    commit_sequencer: SMutex<Option<Arc<dyn CommitSequencer>>>,
    /// Replication-mode control (write gate + sync commit path), installed
    /// when the server runs in stream mode.
    repl_ctl: SMutex<Option<Arc<dyn ReplControl>>>,
    /// The worker's replica-ack waiter (replication-mode primary with
    /// `sync = true`), installed when the worker's replication tasks start.
    repl_ack_waiter: SMutex<Option<Arc<dyn ReplAckWait>>>,
    // commit_gate: AsyncMutex<()>,
}

/// In-memory cursor over a fully materialized row set, returned as the
/// `RSCursor` of a range read.
pub struct VecCursor {
    inner: SMutex<VecCursorInner>,
}

/// Mutable state of a [`VecCursor`]: the materialized rows plus the index of
/// the next row to return.
pub struct VecCursorInner {
    rows: Vec<TupleRow>,
    index: usize,
}

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only contract.
pub type IoUringXContract = WorkerXContract;

/// In-memory vector-backed cursor used as the `RSCursor` for range reads.
pub mod cursor;
/// Worker-local KV operations and the transaction commit paths of
/// `WorkerXContract`.
pub mod kv;
/// Construction, initialization, and replication wiring of `WorkerXContract`.
pub mod lifecycle;
/// Relation DML implementations (insert/read/update/delete) of
/// `WorkerXContract`.
pub mod ops;
/// Parameter bundles for constructing a `WorkerXContract`.
pub mod params;
/// Wire messages exchanged between workers for partition RPC.
pub mod partition_rpc;
/// Partition RPC handler, sender, and the routed commit paths of
/// `WorkerXContract`.
pub mod rpc;
/// Integration tests for the worker contract.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub mod tests;
/// The `XContract` trait implementation for `WorkerXContract`.
pub mod trait_impl;
/// Tuple-building, predicate-matching, and lock helpers shared by the
/// contract modules.
pub mod utils;
/// Per-worker storage: relation indexes, MVCC KV rows, and commit apply.
pub mod worker_storage;
/// Transaction state staging behind the `TxMgr` trait.
pub mod worker_tx_manager;
/// Statement/commit key lock manager with FIFO-fair bounded waiting.
pub mod x_lock_mgr;

pub use params::{WorkerXContractParams, WorkerXContractWorkerLogParams};
