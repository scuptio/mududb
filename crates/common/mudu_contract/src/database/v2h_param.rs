//! `database::v2h_param` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::id::OID;
use mudu_type::data_value::DataValue;
use serde::{Deserialize, Serialize};

pub struct QueryIn {
    xid: OID,
    sql: String,
    param_list: Vec<DataValue>,
    param_desc: TupleFieldDesc,
}

#[derive(Serialize, Deserialize)]
pub struct QueryResult {
    xid: OID,
    tuple_desc: TupleFieldDesc,
}

#[derive(Serialize, Deserialize)]
pub struct ResultCursor {
    xid: OID,
}
#[derive(Serialize, Deserialize)]
pub struct ResultRow {
    result: Option<TupleField>,
}

#[derive(Serialize, Deserialize)]
pub struct CommandIn {
    xid: OID,
    sql: String,
    param_list: Vec<Vec<u8>>,
    param_desc: TupleFieldDesc,
}

#[derive(Serialize, Deserialize)]
pub struct CommandResult {
    xid: OID,
    affected_rows: u64,
}

impl QueryIn {
    pub fn new(xid: OID, sql: String, param: Vec<DataValue>, desc: TupleFieldDesc) -> Self {
        Self {
            xid,
            sql,
            param_list: param,
            param_desc: desc,
        }
    }

    pub fn xid(&self) -> OID {
        self.xid
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn param_list(&self) -> &Vec<DataValue> {
        &self.param_list
    }

    pub fn param_desc(&self) -> &[DatumDesc] {
        self.param_desc.fields()
    }
}

impl ResultCursor {
    pub fn new(xid: OID) -> ResultCursor {
        Self { xid }
    }

    pub fn xid(&self) -> OID {
        self.xid
    }
}
impl QueryResult {
    pub fn new(xid: OID, row_desc: TupleFieldDesc) -> QueryResult {
        Self {
            xid,
            tuple_desc: row_desc,
        }
    }
    pub fn xid(&self) -> OID {
        self.xid
    }

    pub fn result_desc(&self) -> &TupleFieldDesc {
        &self.tuple_desc
    }

    pub fn into_tuple_desc(self) -> TupleFieldDesc {
        self.tuple_desc
    }

    pub fn cursor(&self) -> ResultCursor {
        ResultCursor::new(self.xid)
    }
}

impl ResultRow {
    pub fn new(result: Option<TupleField>) -> ResultRow {
        Self { result }
    }

    pub fn result(&self) -> &Option<TupleField> {
        &self.result
    }

    pub fn into_result(self) -> Option<TupleField> {
        self.result
    }
}

impl CommandIn {
    pub fn new(xid: OID, sql: String, param: Vec<Vec<u8>>, desc: TupleFieldDesc) -> CommandIn {
        Self {
            xid,
            sql,
            param_list: param,
            param_desc: desc,
        }
    }

    pub fn xid(&self) -> OID {
        self.xid
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn param(&self) -> &Vec<Vec<u8>> {
        &self.param_list
    }

    pub fn param_desc(&self) -> &TupleFieldDesc {
        &self.param_desc
    }
}

impl CommandResult {
    pub fn new(xid: OID, affected_rows: u64) -> Self {
        Self { xid, affected_rows }
    }

    pub fn affected_rows(&self) -> u64 {
        self.affected_rows
    }

    pub fn xid(&self) -> OID {
        self.xid
    }
}
