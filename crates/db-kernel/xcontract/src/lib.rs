//! Kernel contract prelude: the worker-side execution contract and every
//! shared type the kernel crates consume.
//!
//! [`WorkerXContract`] is the per-worker implementation of the `XContract`
//! storage engine contract: transactional relation DML and worker-local KV,
//! WAL-backed commit, and cross-partition routing over the worker channel. The
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
pub use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
pub use std::sync::Arc;
pub use std::time::Duration;
pub use tracing::{debug, trace, warn};

pub use ::catalog::contract::meta_mgr::MetaMgr;
pub use ::catalog::contract::schema_class::SchemaClass;
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
    AlterTable, DeltaAssign, Filter, OptDelete, OptInsert, OptRead, OptUpdate, Predicate,
    QueueClaimedRow, QueueConsumerEntry, QueueShard, QueueWaiter, RSCursor, RangeData, TupleRow,
    VecDatum, VecSelTerm, XContract, XContractQueue,
};
pub use ::xengine::partition_router::{PartitionRouter, DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID};
pub use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
pub use mudu::channel::{
    DeliveryMode, Envelope, MessageKind, OutgoingMessage, RecvFilter, ServerInstanceId, ServiceCtx,
};
pub use mudu_sys::contract::async_io_provider::AsyncIoProvider;
pub use mudu_utils::channel::{current_channel, ServiceHandle, ServiceRouter};

/// Thin public-API facade over `WorkerXContract` delegating to the business
/// modules.
pub mod api;
/// Per-worker cursor cache of queue-shard consumers (leaf module).
mod consumer_registry;
/// The per-worker contract type and commit-routing state.
pub mod contract;
/// In-memory vector-backed cursor used as the `RSCursor` for range reads.
pub mod cursor;
/// The local epoch oracle (single-node commit-order source).
pub mod epoch_oracle;
/// Worker-local KV operations and the transaction commit paths of
/// `WorkerXContract`.
pub mod kv;
/// Construction, initialization, and replication wiring of `WorkerXContract`.
pub mod lifecycle;
mod lifecycle_cluster;
/// Wire messages of the management-plane RPC served by workers (queue
/// activation control, queue observability).
pub mod management_rpc;
/// The MetaGroup client face (propose over the cluster channel).
pub mod meta_group;
/// Relation DML implementations (insert/read/update/delete) of
/// `WorkerXContract`.
pub mod ops;
/// Parameter bundles for constructing a `WorkerXContract`.
pub mod params;
/// Wire messages exchanged between workers for partition RPC.
pub mod partition_rpc;
/// Per-worker queue runtime counters (sequence allocation, acked frontier)
/// and the queue operations of `WorkerXContract`.
pub mod queue;
/// Per-worker queue activation registry (volatile post-commit procedure
/// invocations) and the activation operations of `WorkerXContract`.
pub mod queue_activation;
/// Per-worker named queue consumer cursors (fan-out mode) and the consumer
/// operations of `WorkerXContract`.
pub mod queue_consumer;
mod queue_receive;
pub(crate) mod queue_state;
/// Per-worker queue-shard runtime counter registry (leaf module).
pub mod queue_types;
/// Per-worker registry of parked queue `RECEIVE ... WAIT` waiters and the
/// commit-path wakeup of `WorkerXContract`.
pub mod queue_wait_registry;
/// Partition RPC handler, sender, and the routed commit paths of
/// `WorkerXContract`.
pub mod rpc;
/// Receiver-side dedup cache of the partition RPC service (leaf module).
mod rpc_dedup;
/// Integration tests for the worker contract.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub mod tests;
/// The `XContract` trait implementation for `WorkerXContract`.
pub mod trait_impl;
/// Shared leaf types (datum, commit route, cursor) consumed by the business
/// modules.
pub mod types;
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

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn not_serving_leader_error_carries_domain_violation() {
        let err = crate::types::not_serving_leader_error();
        assert_eq!(err.ec(), ErrorCode::DomainViolation);
        assert!(err.to_string().contains("not the serving leader"));
    }
}
