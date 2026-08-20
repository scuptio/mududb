use crate::contract::fs_type::FsTypeKind;
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_table::SchemaTable;
use crate::x_engine::api::{DeltaAssign, OptRead, Predicate, RangeData, VecDatum, VecSelTerm};
use crate::x_engine::tx_mgr::TxMgr;
use mudu::common::id::OID;
use std::sync::Arc;

#[derive(Clone)]
pub struct PAccessKey {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub table_id: OID,
    pub pred_key: VecDatum,
    pub select: VecSelTerm,
    pub opt_read: OptRead,
}

pub struct PAccessRange {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub table_id: OID,
    pub pred_key: RangeData,
    pub pred_non_key: Predicate,
    pub select: VecSelTerm,
    pub opt_read: OptRead,
}

#[derive(Clone)]
pub struct PCreatePartitionRule {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub rule: PartitionRuleDesc,
}

#[derive(Clone)]
pub struct PCreatePartitionPlacement {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub placements: Vec<PartitionPlacement>,
}

#[derive(Clone)]
pub struct PCreateTable {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub schema: SchemaTable,
    pub partition_binding: Option<TablePartitionBinding>,
}

#[derive(Clone)]
pub struct PDropTable {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub oid: Option<OID>,
}

#[derive(Clone)]
pub struct PCreateFsType {
    pub name: String,
    pub kind: FsTypeKind,
}

#[derive(Clone)]
pub struct PDropType {
    pub name: String,
}

#[derive(Clone)]
pub struct PInsertKeyValue {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub table_id: OID,
    pub rows: Vec<(VecDatum, VecDatum)>,
}

#[derive(Clone)]
pub struct PUpdateKeyValue {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub table_id: OID,
    pub key: VecDatum,
    pub value: VecDatum,
    /// Restricted `SET col = col <+|-> <integer literal or ?>` assignments,
    /// evaluated against the latest committed row under the statement lock.
    pub delta_assignments: Vec<DeltaAssign>,
}

#[derive(Clone)]
pub struct PDeleteKeyValue {
    pub tx_mgr: Arc<dyn TxMgr>,
    pub table_id: OID,
    pub key: VecDatum,
}
