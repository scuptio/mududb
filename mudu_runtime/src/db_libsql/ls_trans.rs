use libsql::Transaction;
use libsql::{Row, Rows};
use mudu::common::result::RS;
use mudu::common::xid::OID;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::result_set::ResultSet;
use mudu_utils::oid::new_xid;

use crate::async_utils::blocking;
use libsql::params::IntoParams;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::async_::AMutex;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use std::sync::Arc;

pub struct LSTrans {
    xid: OID,
    trans: Transaction,
}

struct LSResultSet {
    inner: Arc<ResultSetInner>,
}

struct ResultSetInner {
    row: AMutex<Rows>,
    tuple_desc: Arc<TupleFieldDesc>,
}

unsafe impl Send for LSTrans {}
unsafe impl Sync for LSTrans {}

impl LSTrans {
    pub fn new(trans: Transaction) -> LSTrans {
        let xid = new_xid();
        Self { xid, trans }
    }

    pub fn xid(&self) -> OID {
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
            .map_err(|e| mudu_error!(ErrorCode::Database, "query error", e))?;
        let rs = Arc::new(LSResultSet::new(rows, desc));
        Ok(rs)
    }

    pub async fn command(&self, sql: &str, params: impl IntoParams) -> RS<u64> {
        let affected_rows = self
            .trans
            .execute(sql, params)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "command error", e))?;
        Ok(affected_rows)
    }

    pub async fn batch(&self, sql: &str) -> RS<u64> {
        let before = self.trans.total_changes();
        let _ = self
            .trans
            .execute_batch(sql)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "batch error", e))?;
        Ok(self.trans.total_changes().saturating_sub(before))
    }

    pub async fn commit(self) -> RS<()> {
        self.trans
            .commit()
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "commit error", e))?;
        Ok(())
    }

    pub async fn rollback(self) -> RS<()> {
        self.trans
            .rollback()
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "rollback error", e))?;
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
            row: AMutex::new(row),
            tuple_desc,
        }
    }

    async fn async_next(&self) -> RS<Option<TupleValue>> {
        let mut guard = self.row.lock().await;
        let opt_row = guard
            .next()
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "query result next", e))?;
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
        return Err(mudu_error!(
            ErrorCode::FatalInternal,
            "column count mismatch"
        ));
    }
    for (i, desc) in item_desc.iter().enumerate() {
        let n = i as i32;
        let internal = match desc.dat_type_id() {
            DatTypeID::I32 => {
                let val = row
                    .get::<i32>(n)
                    .map_err(|e| mudu_error!(ErrorCode::Database, "get item of row error", e))?;
                DatValue::from_i32(val)
            }
            DatTypeID::I64 => {
                let val = row
                    .get::<i64>(n)
                    .map_err(|e| mudu_error!(ErrorCode::Database, "get item of row error", e))?;
                DatValue::from_i64(val)
            }
            DatTypeID::F32 => {
                let val = row
                    .get::<f64>(n)
                    .map_err(|e| mudu_error!(ErrorCode::Database, "get item of row error", e))?;
                DatValue::from_f64(val)
            }
            DatTypeID::F64 => {
                let val = row
                    .get::<f64>(n)
                    .map_err(|_e| mudu_error!(ErrorCode::Database, "get item of row error"))?;
                DatValue::from_f64(val)
            }
            DatTypeID::String => {
                let val = row
                    .get::<String>(n)
                    .map_err(|e| mudu_error!(ErrorCode::Database, "get item of row error", e))?;
                DatValue::from_string(val)
            }
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    format!(
                        "libsql unsupported type in sync result conversion: {:?}",
                        desc
                    )
                ));
            }
        };

        vec.push(internal)
    }
    Ok(TupleValue::from(vec))
}

#[cfg(test)]
#[path = "ls_trans_test.rs"]
mod ls_trans_test;
