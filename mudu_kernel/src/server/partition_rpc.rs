use crate::wal::xl_data_op::XLWrite;
use mudu::common::id::{AttrIndex, OID};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum RpcBound {
    Included(Vec<u8>),
    Excluded(Vec<u8>),
    Unbounded,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PartitionRpcRequest {
    ReadKey {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    },
    ReadRange {
        table_id: OID,
        partition_id: OID,
        start: RpcBound,
        end: RpcBound,
        select: Vec<AttrIndex>,
    },
    Insert {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
    },
    Update {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        values: Vec<(AttrIndex, Vec<u8>)>,
    },
    ApplyCrossPartitionTx {
        tx_id: OID,
        coordinator_worker_id: OID,
        partition_id: OID,
        visibility_epoch: u64,
        partition_write_set: Vec<XLWrite>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PartitionRpcResponse {
    ReadKey(Option<Vec<Option<Vec<u8>>>>),
    ReadRange(Vec<Vec<Option<Vec<u8>>>>),
    Insert,
    Delete(usize),
    Update(usize),
    ApplyCrossPartitionTx,
    Err(String),
}
