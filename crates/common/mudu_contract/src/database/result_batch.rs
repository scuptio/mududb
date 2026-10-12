//! A batch of result rows collected from a result set.

use crate::database::result_set::{ResultSet, ResultSetAsync};
use crate::tuple::tuple_value::TupleValue;
use mudu::common::id::OID;
use mudu::common::result::RS;

/// A batch of result rows with an end-of-stream marker.
pub struct ResultBatch {
    oid: OID,
    eof: bool,
    rows: Vec<TupleValue>,
}

impl ResultBatch {
    /// Drains an async result set into a batch marked as end-of-stream.
    pub async fn from_result_set_async(oid: OID, rs: &dyn ResultSetAsync) -> RS<Self> {
        let mut vec = Vec::new();
        while let Some(n) = rs.next().await? {
            vec.push(n);
        }
        Ok(Self {
            oid,
            eof: true,
            rows: vec,
        })
    }

    /// Drains a result set into a batch marked as end-of-stream.
    pub fn from_result_set(oid: OID, rs: &dyn ResultSet) -> RS<Self> {
        let mut vec = Vec::new();
        while let Some(n) = rs.next()? {
            vec.push(n);
        }
        Ok(Self {
            oid,
            eof: true,
            rows: vec,
        })
    }

    /// Creates a batch from explicit rows and an end-of-stream flag.
    pub fn from(oid: OID, rows: Vec<TupleValue>, eof: bool) -> ResultBatch {
        ResultBatch { oid, eof, rows }
    }

    /// Creates an empty batch, not yet at end-of-stream.
    pub fn new(oid: OID) -> ResultBatch {
        Self {
            oid,
            eof: false,
            rows: Vec::new(),
        }
    }

    /// Returns the object id owning this batch.
    pub fn oid(&self) -> OID {
        self.oid
    }

    /// Returns whether the batch carries the end of the result stream.
    pub fn is_eof(&self) -> bool {
        self.eof
    }

    /// Consumes the batch and returns its rows.
    pub fn into_rows(self) -> Vec<TupleValue> {
        self.rows
    }

    /// Returns the rows of the batch.
    pub fn rows(&self) -> &Vec<TupleValue> {
        &self.rows
    }

    /// Returns a mutable reference to the rows of the batch.
    pub fn mut_rows(&mut self) -> &mut Vec<TupleValue> {
        &mut self.rows
    }

    /// Appends a row to the batch.
    pub fn add_row(&mut self, row: TupleValue) {
        self.rows.push(row);
    }

    /// Marks the batch as end-of-stream.
    pub fn set_eof(&mut self) {
        self.eof = true;
    }
}
