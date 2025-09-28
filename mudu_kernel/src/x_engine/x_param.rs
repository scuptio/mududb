use crate::contract::schema_table::SchemaTable;
use crate::x_engine::api::{OptRead, Predicate, RangeData, VecDatum, VecSelTerm};
use mudu::common::id::OID;
use mudu::common::xid::XID;

#[derive(Clone, Debug)]
pub struct PAccessKey {
    pub xid: XID,
    pub table_id: OID,
    pub pred_key: VecDatum,
    pub select: VecSelTerm,
    pub opt_read: OptRead,
}

pub struct PAccessRange {
    pub xid: XID,
    pub table_id: OID,
    pub pred_key: RangeData,
    pub pred_non_key: Predicate,
    pub select: VecSelTerm,
    pub opt_read: OptRead,
}

#[derive(Clone, Debug)]
pub struct PCreateTable {
    pub xid: XID,
    pub schema: SchemaTable,
}

#[derive(Clone, Debug)]
pub struct PDropTable {
    pub xid: XID,
    pub oid: Option<OID>,
}

#[derive(Clone, Debug)]
pub struct PInsertKeyValue {
    pub xid: XID,
    pub table_id: OID,
    pub key: VecDatum,
    pub value: VecDatum,
}

#[derive(Clone, Debug)]
pub struct PUpdateKeyValue {
    pub xid: XID,
    pub table_id: OID,
    pub key: VecDatum,
    pub value: VecDatum,
}
