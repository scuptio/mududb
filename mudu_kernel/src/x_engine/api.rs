use async_trait::async_trait;
use std::ops::Bound;
use std::sync::Arc;

use crate::contract::schema_table::SchemaTable;
use crate::x_engine::dat_bin::DatBin;
use crate::x_engine::operator::Operator;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::tuple::tuple_item::TupleItem;

pub type TupleRow = TupleItem;

/// Result set cursor
#[async_trait]
pub trait RSCursor: Send + Sync {
    async fn next(&self) -> RS<Option<TupleRow>>;
}


pub type Filter = Operator;

/// object id and datum
#[derive(Clone, Default, Debug)]
pub struct VecDatum {
    data: Vec<(OID, DatBin)>,
}

/// range data
#[derive(Clone)]
pub struct RangeData {
    start: Bound<Vec<(OID, DatBin)>>,
    end: Bound<Vec<(OID, DatBin)>>,
}

/// select term list
#[derive(Clone, Debug)]
pub struct VecSelTerm {
    vec: Vec<OID>,
}

/// predicate for non-primary-key
#[derive(Clone, Debug)]
pub enum Predicate {
    /// conjunctive normal form, it is a conjunction of disjunctions of literals
    CNF(Vec<Vec<(OID, Filter)>>),
    /// disjunctive normal form, it is a disjunction of conjunctions of literals
    DNF(Vec<Vec<(OID, Filter)>>),
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
/// MKI trait
///
/// A trait of XContract interface, which is a transaction processing abstraction on relational model
/// All the tables, columns, types, transactions etc. are reference by a unique and immutable object
/// id, [`OID`]
///
#[async_trait]
pub trait XContract: Send + Sync {
    /// create a table, which is described by `schema`
    async fn create_table(&self, xid: XID, schema: &SchemaTable) -> RS<()>;

    /// drop a table specified by its OID
    async fn drop_table(&self, xid: XID, oid: OID) -> RS<()>;

    /// alter table
    async fn alter_table(&self, xid: XID, oid: OID, alter_table: &AlterTable) -> RS<()>;

    /// start a transaction
    async fn begin_tx(&self) -> RS<XID>;

    /// commit a transaction specified by its XID
    async fn commit_tx(&self, xid: XID) -> RS<()>;

    /// abort a transaction specified by its XID
    async fn abort_tx(&self, xid: XID) -> RS<()>;

    /// update by a collection of predicate
    async fn update(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        opt_update: &OptUpdate,
    ) -> RS<usize>;

    /// read by a exact key
    async fn read_key(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Option<Vec<DatBin>>>;

    /// read by a collection of predicate
    async fn read_range(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>>;

    /// delete by a collection of predicate
    async fn delete(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        opt_delete: &OptDelete,
    ) -> RS<usize>;

    /// insert a row
    async fn insert(
        &self,
        xid: XID,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        opt_insert: &OptInsert,
    ) -> RS<()>;
}

impl VecDatum {
    pub fn new(data: Vec<(OID, DatBin)>) -> Self {
        Self { data }
    }

    pub fn swap(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.data, &mut other.data);
    }

    pub fn data(&self) -> &Vec<(OID, DatBin)> {
        &self.data
    }

    pub fn into_data(self) -> Vec<(OID, DatBin)> {
        self.data
    }
}

impl VecSelTerm {
    pub fn new(proj_list: Vec<OID>) -> Self {
        Self { vec: proj_list }
    }

    pub fn vec(&self) -> &Vec<OID> {
        &self.vec
    }
}
