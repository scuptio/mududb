//! Value-to-host query/command parameter and result types used by the
//! procedure host interface.

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::id::OID;
use mudu_type::data_value::DataValue;
use serde::{Deserialize, Serialize};

/// A query issued from inside a procedure: SQL text plus typed parameters.
pub struct QueryIn {
    xid: OID,
    sql: String,
    param_list: Vec<DataValue>,
    param_desc: TupleFieldDesc,
}

/// Handle to a query result cached host-side, with the row description.
#[derive(Serialize, Deserialize)]
pub struct QueryResult {
    xid: OID,
    tuple_desc: TupleFieldDesc,
}

/// Cursor over a query result cached host-side.
#[derive(Serialize, Deserialize)]
pub struct ResultCursor {
    xid: OID,
}

/// One row fetched through a result cursor; `None` marks the end.
#[derive(Serialize, Deserialize)]
pub struct ResultRow {
    result: Option<TupleField>,
}

/// A command issued from inside a procedure: SQL text plus binary-encoded parameters.
#[derive(Serialize, Deserialize)]
pub struct CommandIn {
    xid: OID,
    sql: String,
    param_list: Vec<Vec<u8>>,
    param_desc: TupleFieldDesc,
}

/// Result of a command issued from inside a procedure.
#[derive(Serialize, Deserialize)]
pub struct CommandResult {
    xid: OID,
    affected_rows: u64,
}

impl QueryIn {
    /// Creates a query input from its transaction id, SQL text, parameters, and parameter description.
    pub fn new(xid: OID, sql: String, param: Vec<DataValue>, desc: TupleFieldDesc) -> Self {
        Self {
            xid,
            sql,
            param_list: param,
            param_desc: desc,
        }
    }

    /// Returns the transaction id.
    pub fn xid(&self) -> OID {
        self.xid
    }

    /// Returns the SQL text.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Returns the parameter values.
    pub fn param_list(&self) -> &Vec<DataValue> {
        &self.param_list
    }

    /// Returns the parameter field descriptions.
    pub fn param_desc(&self) -> &[DatumDesc] {
        self.param_desc.fields()
    }
}

impl ResultCursor {
    /// Creates a cursor over the cached result of session/transaction `xid`.
    pub fn new(xid: OID) -> ResultCursor {
        Self { xid }
    }

    /// Returns the transaction id identifying the cached result.
    pub fn xid(&self) -> OID {
        self.xid
    }
}
impl QueryResult {
    /// Creates a query result handle for `xid` with the row description `row_desc`.
    pub fn new(xid: OID, row_desc: TupleFieldDesc) -> QueryResult {
        Self {
            xid,
            tuple_desc: row_desc,
        }
    }

    /// Returns the transaction id identifying the cached result.
    pub fn xid(&self) -> OID {
        self.xid
    }

    /// Returns the result row description.
    pub fn result_desc(&self) -> &TupleFieldDesc {
        &self.tuple_desc
    }

    /// Consumes `self` and returns the result row description.
    pub fn into_tuple_desc(self) -> TupleFieldDesc {
        self.tuple_desc
    }

    /// Returns a cursor over this result.
    pub fn cursor(&self) -> ResultCursor {
        ResultCursor::new(self.xid)
    }
}

impl ResultRow {
    /// Creates a result row; `None` marks the end of the result set.
    pub fn new(result: Option<TupleField>) -> ResultRow {
        Self { result }
    }

    /// Returns the row, or `None` at the end of the result set.
    pub fn result(&self) -> &Option<TupleField> {
        &self.result
    }

    /// Consumes `self` and returns the row, or `None` at the end of the result set.
    pub fn into_result(self) -> Option<TupleField> {
        self.result
    }
}

impl CommandIn {
    /// Creates a command input from its transaction id, SQL text, binary parameters, and parameter description.
    pub fn new(xid: OID, sql: String, param: Vec<Vec<u8>>, desc: TupleFieldDesc) -> CommandIn {
        Self {
            xid,
            sql,
            param_list: param,
            param_desc: desc,
        }
    }

    /// Returns the transaction id.
    pub fn xid(&self) -> OID {
        self.xid
    }

    /// Returns the SQL text.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Returns the binary-encoded parameter values.
    pub fn param(&self) -> &Vec<Vec<u8>> {
        &self.param_list
    }

    /// Returns the parameter description.
    pub fn param_desc(&self) -> &TupleFieldDesc {
        &self.param_desc
    }
}

impl CommandResult {
    /// Creates a command result from its transaction id and affected-row count.
    pub fn new(xid: OID, affected_rows: u64) -> Self {
        Self { xid, affected_rows }
    }

    /// Returns the number of affected rows.
    pub fn affected_rows(&self) -> u64 {
        self.affected_rows
    }

    /// Returns the transaction id.
    pub fn xid(&self) -> OID {
        self.xid
    }
}
