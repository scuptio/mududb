use crate::contract::WorkerXContract;
use crate::partition_rpc::{PartitionRpcRequest, PartitionRpcResponse, RpcBound};
use crate::queue_types::shard_key;
use crate::types::{not_serving_leader_error, DataBin, ReplCommitRoute};
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::commit_ts::next_commit_timestamp;
use ::wal::worker_log::WorkerLogBackend;
use ::wal::xl_batch::XLBatch;
use ::wal::xl_data_op::{XLDelete, XLInsert, XLWrite};
use ::wal::xl_entry::{TxOp, XLEntry};
use ::xengine::api::{QueueShard, VecDatum, VecSelTerm};
use ::xengine::partition_router::DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use mudu::common::buf::Buf;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_utils::channel::current_channel;
use mudu_utils::gen_oid;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

#[cfg(test)]
use crate::params::WorkerXContractParams;
#[cfg(test)]
use crate::x_lock_mgr::XLockMgr;
#[cfg(test)]
use ::catalog::contract::table_desc::TableDesc;
#[cfg(test)]
use ::catalog::repl_ctrl::RaftShareView;
#[cfg(test)]
use ::storage::prepared_commit::CommitSequencer;
#[cfg(test)]
use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogLayout};
#[cfg(test)]
use ::xengine::api::XContract;
#[cfg(test)]
use ::xengine::api::{DeltaAssign, DeltaOp, OptDelete, OptInsert, OptRead, Predicate, RangeData};
#[cfg(test)]
use mudu::channel::ServiceCtx;
#[cfg(test)]
use std::sync::atomic::AtomicU64;

mod client;
mod client_migration;
mod client_routing;
mod client_two_phase;
mod coordinator;
pub(crate) mod coordinator_client;
mod coordinator_query;
mod coordinator_recovery;
pub(crate) mod cross_partition;
mod participant_wal;
mod serve;
mod serve_migration;
mod serve_queue;
mod serve_two_phase;
mod serve_write;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct CrossPartitionParticipant {
    partition_id: OID,
    worker_id: OID,
}

fn cross_partition_wal_ops(write_set: &[XLWrite]) -> Vec<TxOp> {
    let mut ops = Vec::with_capacity(write_set.len() + 2);
    ops.push(TxOp::Begin);
    ops.extend(write_set.iter().cloned().map(TxOp::Write));
    ops.push(TxOp::Commit(next_commit_timestamp()));
    ops
}

fn partition_write_set(write_set: &[XLWrite], partition_id: OID) -> Vec<XLWrite> {
    write_set
        .iter()
        .filter(|write| write.partition_id() == partition_id)
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "rpc_test.rs"]
mod rpc_test;
