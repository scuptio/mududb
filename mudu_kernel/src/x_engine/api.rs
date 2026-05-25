use async_trait::async_trait;
use std::ops::Bound;
use std::sync::Arc;

use crate::contract::schema_table::SchemaTable;
use crate::x_engine::dat_bin::DatBin;
use crate::x_engine::operator::Operator;
use crate::x_engine::tx_mgr::TxMgr;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field::TupleField;

pub type TupleRow = TupleField;

/// Asynchronous cursor over a result set produced by [`XContract::read_range`].
#[async_trait]
pub trait RSCursor: Send + Sync {
    /// Returns the next projected row, or `None` when the cursor is exhausted.
    async fn next(&self) -> RS<Option<TupleRow>>;
}

pub type Filter = Operator;

/// A compact row fragment keyed by attribute index.
///
/// The contract uses this type for exact-key predicates, inserted key/value
/// columns, and update payloads. Each pair is `(attribute_index, binary_value)`.
#[derive(Clone, Default, Debug)]
pub struct VecDatum {
    data: Vec<(AttrIndex, DatBin)>,
}

/// Key-range bounds used by [`XContract::read_range`].
///
/// Bounds are expressed over the same `(attribute_index, binary_value)` shape as
/// [`VecDatum`], but allow inclusive, exclusive, or unbounded range scans.
#[derive(Clone)]
pub struct RangeData {
    start: Bound<Vec<(AttrIndex, DatBin)>>,
    end: Bound<Vec<(AttrIndex, DatBin)>>,
}

/// Projection list for read operations.
#[derive(Clone, Debug)]
pub struct VecSelTerm {
    vec: Vec<AttrIndex>,
}

/// Predicate over non-key columns.
#[derive(Clone, Debug)]
pub enum Predicate {
    /// conjunctive normal form, it is a conjunction of disjunctions of literals
    CNF(Vec<Vec<(AttrIndex, Filter)>>),
    /// disjunctive normal form, it is a disjunction of conjunctions of literals
    DNF(Vec<Vec<(AttrIndex, Filter)>>),
    /// equality over a left prefix of the primary key, evaluated during range reads
    KeyPrefixEq(Vec<(AttrIndex, DatBin)>),
}

/// alter table parameter
pub enum AlterTable {}

/**
- optional parameter for read operation
 */
#[derive(Clone, Debug, Default)]
pub struct OptRead {}

/**
- optional parameter for update operation
 */
pub struct OptUpdate {}

/**
- optional parameter for insert operation
 */
#[derive(Clone, Debug, Default)]
pub struct OptInsert {}

/**
- optional parameter for delete operation
 */
#[derive(Clone, Default)]
pub struct OptDelete {}

///////////////////////////////////////////////////////////////////////////////
/// Transactional relational execution interface used by the kernel.
///
/// [`XContract`] is the storage-facing contract behind SQL execution and the
/// worker-local runtime. All stable schema objects are addressed by immutable
/// object identifiers such as [`OID`], while each write/read statement is
/// executed inside a transaction identified by a [`TxMgr`] handle.
///
/// Conventions:
/// - `table_id` always identifies the target table by OID.
/// - `pred_key` carries exact primary-key components for point operations.
/// - `pred_non_key` refines the operation with additional non-key predicates.
/// - `select` lists projected columns for read operations.
/// - row-count return values report how many visible rows were affected.
#[async_trait]
pub trait XContract: Send + Sync {
    /// Creates a table described by `schema`.
    ///
    /// `tx_mgr` is accepted for interface uniformity; implementations may treat
    /// DDL as autocommit if transactional DDL is not supported.
    async fn create_table(&self, tx_mgr: Arc<dyn TxMgr>, schema: &SchemaTable) -> RS<()>;

    /// Drops the table identified by `oid`.
    async fn drop_table(&self, tx_mgr: Arc<dyn TxMgr>, oid: OID) -> RS<()>;

    /// Applies an alter-table operation to the target table.
    async fn alter_table(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        oid: OID,
        alter_table: &AlterTable,
    ) -> RS<()>;

    /// Starts a new transaction and returns its transaction manager.
    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>>;

    /// Commits the transaction identified by `tx_mgr`.
    async fn commit_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()>;

    /// Aborts the transaction identified by `tx_mgr`.
    async fn abort_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()>;

    /// Updates rows that match the provided key and non-key predicates.
    ///
    /// Returns the number of visible rows updated.
    async fn update(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        opt_update: &OptUpdate,
    ) -> RS<usize>;

    /// Reads one row by exact key.
    ///
    /// Returns `None` when the key is not visible in the transaction snapshot.
    async fn read_key(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Option<Vec<DatBin>>>;

    /// Reads rows from a key range plus optional non-key predicates.
    ///
    /// The returned cursor yields projected rows in the implementation-defined
    /// order of the range scan.
    async fn read_range(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>>;

    /// Deletes rows that match the provided key and non-key predicates.
    ///
    /// Returns the number of visible rows deleted.
    async fn delete(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        opt_delete: &OptDelete,
    ) -> RS<usize>;

    /// Inserts one row identified by `keys` with payload columns from `values`.
    async fn insert(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        opt_insert: &OptInsert,
    ) -> RS<()>;
}

impl VecDatum {
    pub fn new(data: Vec<(AttrIndex, DatBin)>) -> Self {
        Self { data }
    }

    pub fn swap(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.data, &mut other.data);
    }

    pub fn data(&self) -> &Vec<(AttrIndex, DatBin)> {
        &self.data
    }

    pub fn into_data(self) -> Vec<(AttrIndex, DatBin)> {
        self.data
    }
}

impl RangeData {
    pub fn new(
        start: Bound<Vec<(AttrIndex, DatBin)>>,
        end: Bound<Vec<(AttrIndex, DatBin)>>,
    ) -> Self {
        Self { start, end }
    }

    pub fn start(&self) -> &Bound<Vec<(AttrIndex, DatBin)>> {
        &self.start
    }

    pub fn end(&self) -> &Bound<Vec<(AttrIndex, DatBin)>> {
        &self.end
    }
}

impl VecSelTerm {
    pub fn new(proj_list: Vec<AttrIndex>) -> Self {
        Self { vec: proj_list }
    }

    pub fn vec(&self) -> &Vec<AttrIndex> {
        &self.vec
    }
}
