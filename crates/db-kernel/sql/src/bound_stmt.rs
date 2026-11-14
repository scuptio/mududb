use ::catalog::contract::fs_type::FsTypeKind;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;
use ::xengine::api::DeltaOp;
use mudu::common::id::{AttrIndex, OID};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::data_type_fn_param::DataType;
use sql_parser::ast::expr_operator::ValueCompare;
use sql_parser::ast::stmt_receive::ReceiveWait;
use std::ops::Bound;

/// A fully bound SQL statement: names resolved to OIDs and literals encoded
/// into binary column datums.
#[derive(Clone, Debug)]
pub enum BoundStmt {
    /// A bound query (read-only statement).
    Query(BoundQuery),
    /// A bound DDL or DML command.
    Command(BoundCommand),
}

/// A bound query.
#[derive(Clone, Debug)]
pub enum BoundQuery {
    /// A bound SELECT statement.
    Select(BoundSelect),
    /// A bound queue RECEIVE statement.
    Receive(BoundReceive),
}

/// A bound DDL or DML command.
#[derive(Clone, Debug)]
pub enum BoundCommand {
    /// A bound CREATE PARTITION PLACEMENT statement.
    CreatePartitionPlacement(BoundCreatePartitionPlacement),
    /// A bound CREATE PARTITION RULE statement.
    CreatePartitionRule(BoundCreatePartitionRule),
    /// A bound CREATE TABLE statement.
    CreateTable(BoundCreateTable),
    /// A bound DROP TABLE statement.
    DropTable(BoundDropTable),
    /// A bound CREATE FS TYPE statement.
    CreateFsType(BoundCreateFsType),
    /// A bound DROP TYPE statement.
    DropType(BoundDropType),
    /// A bound CREATE QUEUE statement.
    CreateQueue(BoundCreateQueue),
    /// A bound DROP QUEUE statement.
    DropQueue(BoundDropQueue),
    /// A bound CREATE CONSUMER statement.
    CreateConsumer(BoundCreateConsumer),
    /// A bound DROP CONSUMER statement.
    DropConsumer(BoundDropConsumer),
    /// A bound queue ACK statement.
    Ack(BoundAck),
    /// A bound INSERT statement.
    Insert(BoundInsert),
    /// A bound UPDATE statement.
    Update(BoundUpdate),
    /// A bound DELETE statement.
    Delete(BoundDelete),
    /// A bound COPY FROM statement.
    CopyFrom(BoundCopyFrom),
    /// A bound COPY TO statement.
    CopyTo(BoundCopyTo),
}

/// A bound SELECT statement.
#[derive(Clone, Debug)]
pub struct BoundSelect {
    /// OID of the table being read.
    pub table_id: OID,
    /// Bound select-list items, in output order.
    pub select_items: Vec<BoundSelectItem>,
    /// Descriptor of the result tuples.
    pub tuple_desc: TupleFieldDesc,
    /// Bound key access predicate.
    pub predicate: BoundPredicate,
    /// Residual (non-key) predicates evaluated row-by-row in the executor
    /// layer after the key access.
    pub residual: Vec<BoundResidual>,
}

/// A bound select-list item: either a plain column projection or an
/// aggregate over the whole (filtered) input.
#[derive(Clone, Debug)]
pub enum BoundSelectItem {
    /// A plain column projection.
    Column(BoundSelectColumn),
    /// An aggregate over the whole (filtered) input.
    Aggregate(BoundAggregate),
}

/// A plain column projection in a select list.
#[derive(Clone, Debug)]
pub struct BoundSelectColumn {
    /// Attribute index of the projected column.
    pub attr: AttrIndex,
    /// Column name in the result tuples.
    pub output_name: String,
}

/// Supported aggregate functions (without `GROUP BY`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateFunc {
    /// `COUNT(*)`, counting the input rows.
    Count,
    /// `SUM(col)`, summing the input values.
    Sum,
    /// `AVG(col)`, averaging the input values.
    Avg,
    /// `MIN(col)`, the smallest input value.
    Min,
    /// `MAX(col)`, the largest input value.
    Max,
}

/// A bound aggregate call, e.g. `COUNT(*)` or `SUM(col)`.
#[derive(Clone, Debug)]
pub struct BoundAggregate {
    /// The aggregate function to compute.
    pub func: AggregateFunc,
    /// Argument column attribute; `None` means `COUNT(*)`.
    pub arg: Option<AttrIndex>,
    /// Data type of the aggregate result.
    pub result_type: DataType,
    /// Column name in the result tuples.
    pub output_name: String,
    /// Whether the result can be NULL (true for everything but COUNT: an
    /// empty input set yields NULL).
    pub nullable: bool,
}

/// A residual (non-key) predicate evaluated in the executor layer.
#[derive(Clone, Debug)]
pub struct BoundResidual {
    /// Attribute index of the compared column.
    pub attr: AttrIndex,
    /// Comparison operator applied to the column value and the literal.
    pub op: ValueCompare,
    /// Literal encoded in the column's binary format; `None` means the
    /// literal is NULL, which makes every comparison UNKNOWN.
    pub literal: Option<Vec<u8>>,
}

/// A bound CREATE PARTITION RULE statement.
#[derive(Clone, Debug)]
pub struct BoundCreatePartitionRule {
    /// Descriptor of the partition rule to create.
    pub rule: PartitionRuleDesc,
}

/// A bound CREATE PARTITION PLACEMENT statement.
#[derive(Clone, Debug)]
pub struct BoundCreatePartitionPlacement {
    /// Partition placements to upsert.
    pub placements: Vec<PartitionPlacement>,
}

/// A bound CREATE TABLE statement.
#[derive(Clone, Debug)]
pub struct BoundCreateTable {
    /// Schema of the table to create.
    pub schema: SchemaTable,
    /// Partition binding of the table, when the table is partitioned.
    pub partition_binding: Option<TablePartitionBinding>,
}

/// A bound DROP TABLE statement.
#[derive(Clone, Debug)]
pub struct BoundDropTable {
    /// OID of the table to drop; `None` when the table does not exist and the
    /// statement carried IF EXISTS.
    pub oid: Option<OID>,
}

/// A bound CREATE FS TYPE statement.
#[derive(Clone, Debug)]
pub struct BoundCreateFsType {
    /// Name of the filesystem type to create.
    pub name: String,
    /// Kind of the filesystem type to create.
    pub kind: FsTypeKind,
}

/// A bound DROP TYPE statement.
#[derive(Clone, Debug)]
pub struct BoundDropType {
    /// Name of the filesystem type to drop.
    pub name: String,
}

/// A bound CREATE QUEUE statement.
#[derive(Clone, Debug)]
pub struct BoundCreateQueue {
    /// Schema of the queue to create. Carries `SchemaObjectKind::Queue` and
    /// the queue system columns (key `__seq`; value headers `__lsn`,
    /// `status`, `receive_count`, `__state_hi`, `__state_lo`) ahead of the
    /// payload columns.
    pub schema: SchemaTable,
    /// Sharding binding of the queue, when it carries a
    /// `PARTITION BY GLOBAL RULE ... REFERENCES (...)` clause.
    pub partition_binding: Option<TablePartitionBinding>,
}

/// A bound DROP QUEUE statement.
#[derive(Clone, Debug)]
pub struct BoundDropQueue {
    /// OID of the queue to drop.
    pub oid: OID,
}

/// A bound CREATE CONSUMER statement (`CREATE CONSUMER <consumer> ON
/// <queue>`): registers a named consumer cursor on the queue, switching it to
/// fan-out mode.
#[derive(Clone, Debug)]
pub struct BoundCreateConsumer {
    /// OID of the queue the consumer cursor is registered on.
    pub table_id: OID,
    /// Name of the consumer cursor to register.
    pub consumer: String,
}

/// A bound DROP CONSUMER statement (`DROP CONSUMER <consumer> ON <queue>`).
#[derive(Clone, Debug)]
pub struct BoundDropConsumer {
    /// OID of the queue the consumer cursor is registered on.
    pub table_id: OID,
    /// Name of the consumer cursor to remove.
    pub consumer: String,
}

/// A bound queue RECEIVE statement.
#[derive(Clone, Debug)]
pub struct BoundReceive {
    /// OID of the queue to receive from.
    pub table_id: OID,
    /// Maximum number of messages to claim (at least 1).
    pub count: u64,
    /// The WAIT clause of the statement. The current slice ignores it and
    /// returns immediately with whatever messages are available; the suspend
    /// slice turns it into a park on the queue wait registry.
    pub wait: Option<ReceiveWait>,
    /// The named consumer cursor (`FOR <consumer>`) this receive reads from;
    /// `None` means the default competing cursor.
    pub consumer: Option<String>,
    /// Descriptor of the returned envelope rows (`__seq`, `__lsn`, then the
    /// payload columns).
    pub tuple_desc: TupleFieldDesc,
}

/// A bound queue ACK statement.
#[derive(Clone, Debug)]
pub struct BoundAck {
    /// OID of the queue to acknowledge.
    pub table_id: OID,
    /// Acknowledged sequence number: claimed messages with `__seq <= seq`
    /// are deleted (or moved to the dead-letter queue when poison).
    pub seq: u64,
    /// The named consumer cursor (`FOR <consumer>`) this ack advances;
    /// `None` means the default competing cursor.
    pub consumer: Option<String>,
}

/// A bound INSERT statement.
#[derive(Clone, Debug)]
pub struct BoundInsert {
    /// OID of the table being inserted into.
    pub table_id: OID,
    /// Rows to insert.
    pub rows: Vec<BoundInsertRow>,
}

/// One row of a bound INSERT statement.
#[derive(Clone, Debug)]
pub struct BoundInsertRow {
    /// Key datums encoded in the columns' binary format.
    pub key: Vec<(AttrIndex, Vec<u8>)>,
    /// Value datums encoded in the columns' binary format.
    pub value: Vec<(AttrIndex, Vec<u8>)>,
}

/// A bound UPDATE statement.
#[derive(Clone, Debug)]
pub struct BoundUpdate {
    /// OID of the table being updated.
    pub table_id: OID,
    /// Key datums identifying the row to update, encoded in the columns'
    /// binary format.
    pub key: Vec<(AttrIndex, Vec<u8>)>,
    /// Assignments applied to the row.
    pub value: Vec<(AttrIndex, BoundSetValue)>,
}

/// Value assigned to a column by one `UPDATE ... SET` item.
#[derive(Clone, Debug)]
pub enum BoundSetValue {
    /// Absolute value encoded in the column's binary format. An empty datum
    /// is the fs-column "touched, system-assigned" rebind sentinel.
    Absolute(Vec<u8>),
    /// SQL NULL assigned by `SET col = NULL` (only reachable for nullable
    /// columns; the binder rejects NOT NULL targets).
    Null,
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

/// A bound DELETE statement.
#[derive(Clone, Debug)]
pub struct BoundDelete {
    /// OID of the table being deleted from.
    pub table_id: OID,
    /// Key datums identifying the row to delete, encoded in the columns'
    /// binary format.
    pub key: Vec<(AttrIndex, Vec<u8>)>,
}

/// A bound COPY FROM statement.
#[derive(Clone, Debug)]
pub struct BoundCopyFrom {
    /// Path of the CSV file to load.
    pub file_path: String,
    /// OID of the table being inserted into.
    pub table_id: OID,
    /// CSV column positions of the key columns.
    pub key_index: Vec<usize>,
    /// CSV column positions of the value columns.
    pub value_index: Vec<usize>,
}

/// A bound COPY TO statement.
#[derive(Clone, Debug)]
pub struct BoundCopyTo {
    /// Path of the CSV file the rows are written to.
    pub file_path: String,
    /// OID of the table whose rows are saved.
    pub table_id: OID,
    /// CSV column positions of the key columns.
    pub key_indexing: Vec<usize>,
    /// CSV column positions of the value columns.
    pub value_indexing: Vec<usize>,
}

/// A bound key access predicate of a SELECT statement.
#[derive(Clone, Debug)]
pub enum BoundPredicate {
    /// No predicate: a full table scan.
    True,
    /// Full primary-key equality: a point read.
    KeyEq {
        /// Key datums identifying the row, encoded in the columns' binary
        /// format.
        key: Vec<(AttrIndex, Vec<u8>)>,
    },
    /// Equality on a prefix of the primary key.
    KeyPrefixEq {
        /// Key prefix datums, encoded in the columns' binary format.
        prefix: Vec<(AttrIndex, Vec<u8>)>,
    },
    /// A primary-key range scan.
    KeyRange {
        /// Start of the key range.
        start: Bound<Vec<(AttrIndex, Vec<u8>)>>,
        /// End of the key range.
        end: Bound<Vec<(AttrIndex, Vec<u8>)>>,
    },
}
