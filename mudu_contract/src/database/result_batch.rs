use crate::database::result_set::{ResultSet, ResultSetAsync};
use crate::tuple::tuple_value::TupleValue;
use mudu::common::id::OID;
use mudu::common::result::RS;

pub struct ResultBatch {
    oid: OID,
    eof: bool,
    rows:Vec<TupleValue>,
}

impl ResultBatch {
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
    pub fn from_result_set(oid:OID, rs:&dyn ResultSet) -> RS<Self> {
        let mut vec = Vec::new();
        while let Some(n) = rs.next()? {
            vec.push(n);
        }
        Ok(Self {
            oid,
            eof: true,
            rows:vec
        })
    }

    pub fn from(oid:OID, rows:Vec<TupleValue>, eof:bool) -> ResultBatch {
        ResultBatch {oid,eof,rows}
    }

    pub fn new(oid: OID) -> ResultBatch {
        Self {
            oid,
            eof:false,
            rows:Vec::new(),
        }
    }

    pub fn oid(&self) -> OID {
        self.oid
    }

    pub fn is_eof(&self) -> bool {
        self.eof
    }

    pub fn into_rows(self) -> Vec<TupleValue> {
        self.rows
    }

    pub fn rows(&self) -> &Vec<TupleValue> {
        &self.rows
    }

    pub fn mut_rows(&mut self) -> &mut Vec<TupleValue> {
        &mut self.rows
    }

    pub fn add_row(&mut self, row: TupleValue) {
        self.rows.push(row);
    }

    pub fn set_eof(&mut self) {
        self.eof = true;
    }
}