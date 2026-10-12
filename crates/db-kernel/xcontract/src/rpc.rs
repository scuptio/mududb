use super::*;
use ::wal::commit_ts::next_commit_timestamp;

mod client;
mod cross_partition;
mod serve;

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
