use crate::contract::fs_type::FsTypeKind;
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_table::SchemaTable;
use crate::x_engine::api::DeltaOp;
use mudu::common::id::{AttrIndex, OID};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::data_type_fn_param::DataType;
use sql_parser::ast::expr_operator::ValueCompare;
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
    CreateFsType(BoundCreateFsType),
    DropType(BoundDropType),
    Insert(BoundInsert),
    Update(BoundUpdate),
    Delete(BoundDelete),
    CopyFrom(BoundCopyFrom),
    CopyTo(BoundCopyTo),
}

#[derive(Clone, Debug)]
pub struct BoundSelect {
    pub table_id: OID,
    pub select_items: Vec<BoundSelectItem>,
    pub tuple_desc: TupleFieldDesc,
    pub predicate: BoundPredicate,
    /// Residual (non-key) predicates evaluated row-by-row in the executor
    /// layer after the key access.
    pub residual: Vec<BoundResidual>,
}

/// A bound select-list item: either a plain column projection or an
/// aggregate over the whole (filtered) input.
#[derive(Clone, Debug)]
pub enum BoundSelectItem {
    Column(BoundSelectColumn),
    Aggregate(BoundAggregate),
}

/// A plain column projection in a select list.
#[derive(Clone, Debug)]
pub struct BoundSelectColumn {
    pub attr: AttrIndex,
    pub output_name: String,
}

/// Supported aggregate functions (without `GROUP BY`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

/// A bound aggregate call, e.g. `COUNT(*)` or `SUM(col)`.
#[derive(Clone, Debug)]
pub struct BoundAggregate {
    pub func: AggregateFunc,
    /// Argument column attribute; `None` means `COUNT(*)`.
    pub arg: Option<AttrIndex>,
    pub result_type: DataType,
    pub output_name: String,
    /// Whether the result can be NULL (true for everything but COUNT: an
    /// empty input set yields NULL).
    pub nullable: bool,
}

/// A residual (non-key) predicate evaluated in the executor layer.
#[derive(Clone, Debug)]
pub struct BoundResidual {
    pub attr: AttrIndex,
    pub op: ValueCompare,
    /// Literal encoded in the column's binary format; `None` means the
    /// literal is NULL, which makes every comparison UNKNOWN.
    pub literal: Option<Vec<u8>>,
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
pub struct BoundCreateFsType {
    pub name: String,
    pub kind: FsTypeKind,
}

#[derive(Clone, Debug)]
pub struct BoundDropType {
    pub name: String,
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
    pub value: Vec<(AttrIndex, BoundSetValue)>,
}

/// Value assigned to a column by one `UPDATE ... SET` item.
#[derive(Clone, Debug)]
pub enum BoundSetValue {
    /// Absolute value encoded in the column's binary format. An empty datum
    /// is the fs-column "touched, system-assigned" rebind sentinel.
    Absolute(Vec<u8>),
    /// Restricted expression assignment `SET col = col <+|-> <integer literal
    /// or ?>`, evaluated against the latest committed column value read
    /// under the statement lock (atomic increment/decrement).
    Delta {
        /// Whether to add or subtract the operand.
        op: DeltaOp,
        /// Operand encoded in the column's binary format.
        literal: Vec<u8>,
    },
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
