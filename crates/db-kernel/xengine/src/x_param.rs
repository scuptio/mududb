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
