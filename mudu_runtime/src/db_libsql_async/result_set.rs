use libsql::{Row, Rows};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use mudu_contract::tuple::tuple_value::TupleValue;



pub struct LibSQLAsyncResultSet {
    inner: Arc<ResultSetInner>,
}

pub struct ResultSetInner {
    row: Mutex<Rows>,
    tuple_desc: Arc<TupleFieldDesc>,
}


impl LibSQLAsyncResultSet {
    pub fn new(rows: Rows, desc: Arc<TupleFieldDesc>) -> LibSQLAsyncResultSet {
        let inner = ResultSetInner::new(rows, desc);
        Self {
            inner: Arc::new(inner),
        }
    }
}

#[async_trait]
impl ResultSetAsync for LibSQLAsyncResultSet {
    async fn next(&self) -> RS<Option<TupleValue>> {
        self.inner.async_next().await
    }

    fn desc(&self) -> &TupleFieldDesc {
        &self.inner.tuple_desc.as_ref()
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
                let items = turso_db_row_to_tuple_item(row, self.tuple_desc.fields())?;
                Ok(Some(items))
            }
            None => Ok(None),
        }
    }
}

fn turso_db_row_to_tuple_item(row: Row, item_desc: &[DatumDesc]) -> RS<TupleValue> {
    let mut vec = vec![];
    if row.column_count() as usize != item_desc.len() {
        return Err(m_error!(EC::FatalError, "column count mismatch"));
    }
    for i in 0..item_desc.len() {
        let desc = &item_desc[i];
        let n = i as i32;
        let internal = match desc.dat_type_id() {
            DatTypeID::I32 => {
                let val = row
                    .get::<i32>(n)
                    .map_err(|e| m_error!(EC::DBInternalError, "libsql db get item of row error", e))?;
                DatValue::from_i32(val)
            }
            DatTypeID::I64 => {
                let val = row
                    .get::<i64>(n)
                    .map_err(|e| m_error!(EC::DBInternalError, "libsql db get item of row error", e))?;
                DatValue::from_i64(val)
            }
            DatTypeID::F32 => {
                let val = row
                    .get::<f64>(n)
                    .map_err(|e| m_error!(EC::DBInternalError, "libsql db get item of row error", e))?;
                DatValue::from_f64(val)
            }
            DatTypeID::F64 => {
                let val = row
                    .get::<f64>(n)
                    .map_err(|_e| m_error!(EC::DBInternalError, "libsql db get item of row error"))?;
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
