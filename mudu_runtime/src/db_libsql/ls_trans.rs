use libsql::Transaction;
use libsql::{Row, Rows};
use mudu::common::result::RS;
use mudu::common::xid::{new_xid, XID};
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::result_set::ResultSet;

use crate::async_utils::blocking;
use libsql::params::IntoParams;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use std::sync::Arc;
use tokio::sync::Mutex;
use mudu_contract::tuple::tuple_value::TupleValue;

pub struct LSTrans {
    xid: XID,
    trans: Transaction,
}

struct LSResultSet {
    inner: Arc<ResultSetInner>,
}

struct ResultSetInner {
    row: Mutex<Rows>,
    tuple_desc: Arc<TupleFieldDesc>,
}

unsafe impl Send for LSTrans {}
unsafe impl Sync for LSTrans {}

impl LSTrans {
    pub fn new(trans: Transaction) -> LSTrans {
        let xid = new_xid();
        Self { xid, trans }
    }

    pub fn xid(&self) -> XID {
        self.xid
    }

    pub async fn query(
        &self,
        sql: &str,
        params: impl IntoParams,
        desc: Arc<TupleFieldDesc>,
    ) -> RS<Arc<dyn ResultSet>> {
        let rows = self
            .trans
            .query(sql, params)
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "query error", e))?;
        let rs = Arc::new(LSResultSet::new(rows, desc));
        Ok(rs)
    }

    pub async fn command(&self, sql: &str, params: impl IntoParams) -> RS<u64> {
        let affected_rows = self
            .trans
            .execute(sql, params)
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "command error", e))?;
        Ok(affected_rows)
    }

    pub async fn commit(self) -> RS<()> {
        self.trans
            .commit()
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "commit error", e))?;
        Ok(())
    }

    pub async fn rollback(self) -> RS<()> {
        self.trans
            .rollback()
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "rollback error", e))?;
        Ok(())
    }
}

impl LSResultSet {
    fn new(rows: Rows, desc: Arc<TupleFieldDesc>) -> LSResultSet {
        let inner = ResultSetInner::new(rows, desc);
        Self {
            inner: Arc::new(inner),
        }
    }
}
impl ResultSet for LSResultSet {
    fn next(&self) -> RS<Option<TupleValue>> {
        let inner = self.inner.clone();
        blocking::run_async(async move { inner.async_next().await })?
    }
}

impl ResultSetInner {
    fn new(row: Rows, tuple_desc: Arc<TupleFieldDesc>) -> ResultSetInner {
        Self {
            row: Mutex::new(row),
            tuple_desc,
        }
    }

    async fn async_next(&self) -> RS<Option<TupleValue>> {
        let mut guard = self.row.lock().await;
        let opt_row = guard
            .next()
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "query result next", e))?;
        match opt_row {
            Some(row) => {
                let items = libsql_row_to_tuple_item(row, self.tuple_desc.fields())?;
                Ok(Some(items))
            }
            None => Ok(None),
        }
    }
}

fn libsql_row_to_tuple_item(row: Row, item_desc: &[DatumDesc]) -> RS<TupleValue> {
    let mut vec = vec![];
    if row.column_count() != (item_desc.len() as i32) {
        return Err(m_error!(EC::FatalError, "column count mismatch"));
    }
    for i in 0..item_desc.len() {
        let desc = &item_desc[i];
        let n = i as i32;
        let internal = match desc.dat_type_id() {
            DatTypeID::I32 => {
                let val = row
                    .get::<i32>(n)
                    .map_err(|e| m_error!(EC::DBInternalError, "get item of row error", e))?;
                DatValue::from_i32(val)
            }
            DatTypeID::I64 => {
                let val = row
                    .get::<i64>(n)
                    .map_err(|e| m_error!(EC::DBInternalError, "get item of row error", e))?;
                DatValue::from_i64(val)
            }
            DatTypeID::F32 => {
                let val = row
                    .get::<f64>(n)
                    .map_err(|e| m_error!(EC::DBInternalError, "get item of row error", e))?;
                DatValue::from_f64(val)
            }
            DatTypeID::F64 => {
                let val = row
                    .get::<f64>(n)
                    .map_err(|_e| m_error!(EC::DBInternalError, "get item of row error"))?;
                DatValue::from_f64(val)
            }
            DatTypeID::String => {
                let val = row
                    .get::<String>(n)
                    .map_err(|e| m_error!(EC::DBInternalError, "get item of row error", e))?;
                DatValue::from_string(val)
            }
            _ => {
                panic!("unsupported type {:?}", desc);
            }
        };

        vec.push(internal)
    }
    Ok(TupleValue::from(vec))
}
