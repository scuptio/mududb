use crate::api::{DeltaAssign, OptRead, Predicate, RangeData, VecDatum, VecSelTerm};
use crate::tx_mgr::TxMgr;
use ::catalog::contract::fs_type::FsTypeKind;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;
use mudu::common::id::{AttrIndex, OID};
use std::sync::Arc;

/// Parameters of a point read by exact primary key.
#[derive(Clone)]
pub struct PAccessKey {
    /// Transaction the read runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the table to read.
    pub table_id: OID,
    /// Exact primary-key values selecting the row.
    pub pred_key: VecDatum,
    /// Columns to project.
    pub select: VecSelTerm,
    /// Read options.
    pub opt_read: OptRead,
}

/// Parameters of a range read.
pub struct PAccessRange {
    /// Transaction the read runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the table to read.
    pub table_id: OID,
    /// Key-range bounds of the scan.
    pub pred_key: RangeData,
    /// Predicate over non-key columns refining the scan.
    pub pred_non_key: Predicate,
    /// Columns to project.
    pub select: VecSelTerm,
    /// Read options.
    pub opt_read: OptRead,
}

/// Parameters of a create-partition-rule command.
#[derive(Clone)]
pub struct PCreatePartitionRule {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// Partition rule to create.
    pub rule: PartitionRuleDesc,
}

/// Parameters of a create-partition-placement command.
#[derive(Clone)]
pub struct PCreatePartitionPlacement {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// Partition-to-worker placements to register.
    pub placements: Vec<PartitionPlacement>,
}

/// Parameters of a create-table command.
#[derive(Clone)]
pub struct PCreateTable {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// Schema of the table to create.
    pub schema: SchemaTable,
    /// Binding of the table to a partition rule, when the table is
    /// partitioned.
    pub partition_binding: Option<TablePartitionBinding>,
}

/// Parameters of a drop-table command.
#[derive(Clone)]
pub struct PDropTable {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the table to drop; `None` makes the command a no-op.
    pub oid: Option<OID>,
}

/// Parameters of a create-queue command.
#[derive(Clone)]
pub struct PCreateQueue {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// Schema of the queue to create; carries the queue kind and the queue
    /// system columns ahead of the payload columns.
    pub schema: SchemaTable,
    /// Sharding binding of the queue, when it is partitioned.
    pub partition_binding: Option<TablePartitionBinding>,
}

/// Parameters of a drop-queue command.
#[derive(Clone)]
pub struct PDropQueue {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the queue to drop.
    pub oid: OID,
}

/// Parameters of a queue `RECEIVE` statement.
#[derive(Clone)]
pub struct PReceiveQueue {
    /// Transaction the receive runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the queue to receive from.
    pub table_id: OID,
    /// Maximum number of messages to claim (at least 1).
    pub count: u64,
    /// The WAIT clause of the statement: `None` returns immediately with
    /// whatever messages are available; `Some` parks on the queue wait
    /// registry between short claim transactions until the batch fills up or
    /// the wait expires (autocommit only, enforced by the worker SQL layer).
    pub wait: Option<::catalog::contract::queue::QueueWait>,
    /// The named consumer cursor (`FOR <consumer>`) this receive reads from;
    /// `None` means the default competing cursor. A named cursor receive is
    /// a read-only scan past the cursor frontier: it claims nothing.
    pub consumer: Option<String>,
    /// Descriptor of the returned envelope rows (`__seq`, `__lsn`, then the
    /// payload columns).
    pub tuple_desc: mudu_contract::tuple::tuple_field_desc::TupleFieldDesc,
}

/// Parameters of a queue `ACK` statement.
#[derive(Clone)]
pub struct PAckQueue {
    /// Transaction the ack runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the queue to acknowledge.
    pub table_id: OID,
    /// Acknowledged sequence number: claimed messages with `__seq <= seq`
    /// are deleted (or moved to the dead-letter queue when poison).
    pub seq: u64,
    /// The named consumer cursor (`FOR <consumer>`) this ack advances;
    /// `None` means the default competing cursor. A named cursor ack moves
    /// the cursor frontier forward and garbage-collects the queue prefix
    /// every consumer of the queue has acknowledged.
    pub consumer: Option<String>,
}

/// Parameters of a create-consumer command (`CREATE CONSUMER cg ON q`).
#[derive(Clone)]
pub struct PCreateConsumer {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the queue the consumer cursor is registered on.
    pub table_id: OID,
    /// Name of the consumer cursor to register.
    pub consumer: String,
}

/// Parameters of a drop-consumer command (`DROP CONSUMER cg ON q`).
#[derive(Clone)]
pub struct PDropConsumer {
    /// Transaction the command runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the queue the consumer cursor is registered on.
    pub table_id: OID,
    /// Name of the consumer cursor to remove.
    pub consumer: String,
}

/// Parameters of a create-filesystem-type command.
#[derive(Clone)]
pub struct PCreateFsType {
    /// Name of the filesystem type to create.
    pub name: String,
    /// Kind of the filesystem type to create.
    pub kind: FsTypeKind,
}

/// Parameters of a drop-filesystem-type command.
#[derive(Clone)]
pub struct PDropType {
    /// Name of the filesystem type to drop.
    pub name: String,
}

/// Parameters of an insert command.
#[derive(Clone)]
pub struct PInsertKeyValue {
    /// Transaction the insert runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the table to insert into.
    pub table_id: OID,
    /// Rows to insert as `(key columns, value columns)` pairs.
    pub rows: Vec<(VecDatum, VecDatum)>,
}

/// Parameters of an update command.
#[derive(Clone)]
pub struct PUpdateKeyValue {
    /// Transaction the update runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the table to update.
    pub table_id: OID,
    /// Exact primary-key values selecting the row.
    pub key: VecDatum,
    /// Absolute column assignments (`SET col = <value>`).
    pub value: VecDatum,
    /// Restricted `SET col = col <+|-> <integer literal or ?>` assignments,
    /// evaluated against the latest committed row under the statement lock.
    pub delta_assignments: Vec<DeltaAssign>,
    /// Columns assigned SQL NULL by `SET col = NULL`.
    pub null_assignments: Vec<AttrIndex>,
}

/// Parameters of a delete command.
#[derive(Clone)]
pub struct PDeleteKeyValue {
    /// Transaction the delete runs in.
    pub tx_mgr: Arc<dyn TxMgr>,
    /// OID of the table to delete from.
    pub table_id: OID,
    /// Exact primary-key values selecting the row.
    pub key: VecDatum,
}
