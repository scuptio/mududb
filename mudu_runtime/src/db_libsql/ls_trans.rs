use libsql::{Connection, Row, Rows};
use libsql::Transaction;
use mudu::common::result::RS;
use mudu::common::xid::{new_xid, XID};
use mudu::database::result_set::ResultSet;
use mudu::error::ec::EC;
use mudu::m_error;

use std::sync::Arc;
use libsql::params::IntoParams;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio::task::block_in_place;
use mudu::data_type::dt_impl::dat_type_id::DatTypeID;
use mudu::data_type::dt_impl::dat_typed::DatTyped;
use mudu::tuple::datum_desc::DatumDesc;
use mudu::tuple::tuple_item::TupleItem;
use mudu::tuple::tuple_item_desc::TupleItemDesc;

pub struct LSTrans {
    xid: XID,
    conn: Connection,
    trans: Transaction,
}

struct LSResultSet {
    inner:Arc<ResultSetInner>,
}

struct ResultSetInner {
    row:Mutex<Rows>,
    tuple_desc: Arc<TupleItemDesc>,
}

impl LSTrans {
    pub fn new(conn: Connection, trans: Transaction) -> LSTrans {
        let xid = new_xid();
        Self { xid, conn, trans }
    }

    pub fn xid(&self) -> XID {
        self.xid
    }

    pub async fn query(
        &self,
        sql: &str,
        params: impl IntoParams,
        desc:Arc<TupleItemDesc>,
    ) -> RS<Arc<dyn ResultSet>> {
        let rows = self.trans.query(
            sql,
            params,
        ).await.map_err(|e| {
            m_error!(EC::DBInternalError, "query error", e)
        })?;
        let rs = Arc::new(LSResultSet::new(rows, desc));
        Ok(rs)
    }

    pub async fn command(&self,
        sql: &str,
        params: impl IntoParams) -> RS<u64> {
        let affected_rows = self.trans.execute(sql, params).await.map_err(|e| {
            m_error!(EC::DBInternalError, "query error", e)
        })?;
        Ok(affected_rows)
    }
    
    pub async fn commit(self) -> RS<()> {
        self.trans.commit().await.map_err(|e|{
            m_error!(EC::DBInternalError, "commit error", e)
        })?;
        Ok(())
    }

    pub async fn rollback(self) -> RS<()> {
        self.trans.rollback().await.map_err(|e|{
            m_error!(EC::DBInternalError, "rollback error", e)
        })?;
        Ok(())
    }
}


impl LSResultSet {
    fn new(rows:Rows, desc:Arc<TupleItemDesc>) -> LSResultSet {
        let inner = ResultSetInner::new(rows, desc);
        Self {
            inner:Arc::new(inner),
        }
    }
}
impl ResultSet for LSResultSet {
    fn next(&self) -> RS<Option<TupleItem>> {
        let inner = self.inner.clone();
        let r = block_in_place( move || {
            Handle::current().block_on(async move {
                inner.async_next().await
            })
        });
        r
    }
}

impl ResultSetInner {
    fn new(row:Rows, tuple_desc: Arc<TupleItemDesc>) -> ResultSetInner {
        Self { row: Mutex::new(row), tuple_desc }
    }

    async fn async_next(&self) -> RS<Option<TupleItem>> {
        let mut guard = self.row.lock().await;
        let opt_row = guard.next().await
            .map_err(|e|{
                m_error!(EC::DBInternalError, "query error", e)
            })?;
        match opt_row {
            Some(row) => {
                let items = libsql_row_to_tuple_item(row, self.tuple_desc.vec_datum_desc())?;
                Ok(Some(items))
            }
            None => { Ok(None) }
        }
    }
}

fn libsql_row_to_tuple_item(row:Row, item_desc:&[DatumDesc]) -> RS<TupleItem> {
    let mut vec = vec![];
    if row.column_count() != (item_desc.len() as i32) {
       return Err(m_error!(EC::FatalError, "column count mismatch"));
    }
    for i in 0..item_desc.len() {
        let desc = &item_desc[i];
        let n = i as i32;
        let dat_typed = match desc.dat_type_id() {
            DatTypeID::I32 => {
                let val = row.get::<i32>(n)
                    .map_err(|e| { m_error!(EC::DBInternalError, "get item of row error") })?;
                DatTyped::I32(val)
            }
            DatTypeID::I64 => {
                let val = row.get::<i64>(n)
                    .map_err(|e| { m_error!(EC::DBInternalError, "get item of row error") })?;
                DatTyped::I64(val)
            }
            DatTypeID::F32 => {
                let val = row.get::<f64>(n)
                    .map_err(|e| { m_error!(EC::DBInternalError, "get item of row error") })?;
                DatTyped::F32(val as _)
            }
            DatTypeID::F64 => {
                let val = row.get::<f64>(n)
                    .map_err(|e| { m_error!(EC::DBInternalError, "get item of row error") })?;
                DatTyped::F64(val)
            }
            DatTypeID::CharVarLen|DatTypeID::CharFixedLen => {
                let val = row.get::<String>(n)
                    .map_err(|e| { m_error!(EC::DBInternalError, "get item of row error") })?;
                DatTyped::String(val)
            }
        };
        let internal = desc.dat_type_id().fn_from_typed()(&dat_typed, desc.dat_type_param())
            .map_err(|e|{m_error!(EC::ConvertErr, "convert data error", e) })?;
        let binary = desc.dat_type_id().fn_send()(&internal, desc.dat_type_param())
        .map_err(|e|{m_error!(EC::ConvertErr, "convert data error", e) })?;
        vec.push(binary.into())
    }
    Ok(TupleItem::new(vec))
}
