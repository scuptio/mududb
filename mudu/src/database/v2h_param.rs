use crate::common::xid::XID;
use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_item::TupleItem;
use crate::tuple::tuple_item_desc::TupleItemDesc;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct QueryIn {
    xid: XID,
    sql: String,
    param: Vec<Vec<u8>>,
    desc: TupleItemDesc,
}

#[derive(Serialize, Deserialize)]
pub struct QueryResult {
    xid: XID,
    tuple_desc: TupleItemDesc,
}

#[derive(Serialize, Deserialize)]
pub struct ResultCursor {
    xid: XID,
}
#[derive(Serialize, Deserialize)]
pub struct ResultRow {
    result: Option<TupleItem>,
}

#[derive(Serialize, Deserialize)]
pub struct CommandIn {
    xid: XID,
    sql: String,
    param: Vec<Vec<u8>>,
    desc: TupleItemDesc,
}

#[derive(Serialize, Deserialize)]
pub struct CommandOut {
    affected_rows: u64,
}


impl QueryIn {
    pub fn new(xid: XID, sql: String, param: Vec<Vec<u8>>, desc: TupleItemDesc) -> Self {
        Self {
            xid,
            sql,
            param,
            desc,
        }
    }

    pub fn xid(&self) -> XID {
        self.xid
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn param(&self) -> &Vec<Vec<u8>> {
        &self.param
    }

    pub fn param_desc(&self) -> &Vec<DatumDesc> {
        &self.desc.vec_datum_desc()
    }
}


impl ResultCursor {
    pub fn new(xid: XID) -> ResultCursor {
        Self {
            xid,
        }
    }

    pub fn xid(&self) -> XID {
        self.xid
    }
}
impl QueryResult {
    pub fn new(xid: XID, row_desc: TupleItemDesc) -> QueryResult {
        Self {
            xid,
            tuple_desc: row_desc,
        }
    }
    pub fn xid(&self) -> XID {
        self.xid
    }

    pub fn row_desc(&self) -> &TupleItemDesc {
        &self.tuple_desc
    }

    pub fn into_tuple_desc(self) -> TupleItemDesc {
        self.tuple_desc
    }


    pub fn cursor(&self) -> ResultCursor {
        ResultCursor::new(self.xid)
    }
}

impl ResultRow {
    pub fn new(result: Option<TupleItem>) -> ResultRow {
        Self {
            result,
        }
    }

    pub fn result(&self) -> &Option<TupleItem> {
        &self.result
    }

    pub fn into_result(self) -> Option<TupleItem> {
        self.result
    }
}


impl CommandIn {
    pub fn new(xid: XID, sql: String, param: Vec<Vec<u8>>, desc: TupleItemDesc) -> CommandIn {
        Self {
            xid,
            sql,
            param,
            desc,
        }
    }

    pub fn xid(&self) -> XID {
        self.xid
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn param(&self) -> &Vec<Vec<u8>> {
        &self.param
    }
}

impl CommandOut {
    pub fn new(affected_rows: u64) -> Self {
        Self {
            affected_rows,
        }
    }

    pub fn affected_rows(&self) -> u64 {
        self.affected_rows
    }
}
