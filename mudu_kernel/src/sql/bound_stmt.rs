use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_table::SchemaTable;
use mudu::common::id::{AttrIndex, OID};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use std::ops::Bound;

#[derive(Clone, Debug)]
pub enum BoundStmt {
    Query(BoundQuery),
    Command(BoundCommand),
}

#[derive(Clone, Debug)]
pub enum BoundQuery {
    Select(BoundSelect),
}

#[derive(Clone, Debug)]
pub enum BoundCommand {
    CreatePartitionPlacement(BoundCreatePartitionPlacement),
    CreatePartitionRule(BoundCreatePartitionRule),
    CreateTable(BoundCreateTable),
    DropTable(BoundDropTable),
    Insert(BoundInsert),
    Update(BoundUpdate),
    Delete(BoundDelete),
    CopyFrom(BoundCopyFrom),
    CopyTo(BoundCopyTo),
}

#[derive(Clone, Debug)]
pub struct BoundSelect {
    pub table_id: OID,
    pub select_attrs: Vec<AttrIndex>,
    pub tuple_desc: TupleFieldDesc,
    pub predicate: BoundPredicate,
}

#[derive(Clone, Debug)]
pub struct BoundCreatePartitionRule {
    pub rule: PartitionRuleDesc,
}

#[derive(Clone, Debug)]
pub struct BoundCreatePartitionPlacement {
    pub placements: Vec<PartitionPlacement>,
}

#[derive(Clone, Debug)]
pub struct BoundCreateTable {
    pub schema: SchemaTable,
    pub partition_binding: Option<TablePartitionBinding>,
}

#[derive(Clone, Debug)]
pub struct BoundDropTable {
    pub oid: Option<OID>,
}

#[derive(Clone, Debug)]
pub struct BoundInsert {
    pub table_id: OID,
    pub rows: Vec<BoundInsertRow>,
}

#[derive(Clone, Debug)]
pub struct BoundInsertRow {
    pub key: Vec<(AttrIndex, Vec<u8>)>,
    pub value: Vec<(AttrIndex, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub struct BoundUpdate {
    pub table_id: OID,
    pub key: Vec<(AttrIndex, Vec<u8>)>,
    pub value: Vec<(AttrIndex, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub struct BoundDelete {
    pub table_id: OID,
    pub key: Vec<(AttrIndex, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub struct BoundCopyFrom {
    pub file_path: String,
    pub table_id: OID,
    pub key_index: Vec<usize>,
    pub value_index: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct BoundCopyTo {
    pub file_path: String,
    pub table_id: OID,
    pub key_indexing: Vec<usize>,
    pub value_indexing: Vec<usize>,
}

#[derive(Clone, Debug)]
pub enum BoundPredicate {
    True,
    KeyEq {
        key: Vec<(AttrIndex, Vec<u8>)>,
    },
    KeyPrefixEq {
        prefix: Vec<(AttrIndex, Vec<u8>)>,
    },
    KeyRange {
        start: Bound<Vec<(AttrIndex, Vec<u8>)>>,
        end: Bound<Vec<(AttrIndex, Vec<u8>)>>,
    },
}
