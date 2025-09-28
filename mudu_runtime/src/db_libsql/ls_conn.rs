use crate::resolver::schema_mgr::SchemaMgr;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::db_conn::DBConn;
use mudu::database::result_set::ResultSet;
use mudu::database::sql_stmt::SQLStmt;
use mudu::tuple::datum::DatumDyn;
use mudu::tuple::tuple_item_desc::TupleItemDesc;
use std::sync::Arc;
use crate::sql_prepare::sql_prepare::SQLPrepare;

pub fn create_ls_conn(conn_str: &String, ddl_path: &String) -> RS<Arc<dyn DBConn>> {
    Ok(Arc::new(LSConn::new(conn_str, ddl_path)?))
}
struct LSConn {
    inner: Arc<LSConnInner>,
}

struct LSConnInner {
    sql_prepare: SQLPrepare,
}

impl LSConn {
    fn new(conn_str: &String, ddl_path: &String) -> RS<Self> {
        let inner = LSConnInner::new(conn_str, ddl_path)?;
        Ok(Self {
            inner: Arc::new(inner)
        })
    }
}

impl LSConnInner {
    fn new(db_path: &String, ddl_path: &String) -> RS<LSConnInner> {
        let sql_prepare = SQLPrepare::new(ddl_path)?;
        Ok(Self {
            sql_prepare
        })
    }

    fn query(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)> {
        todo!()
    }

    fn command(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<usize> {
        todo!()
    }
}

impl DBConn for LSConn {
    fn begin_tx(&self) -> RS<XID> {
        todo!()
    }

    fn rollback_tx(&self) -> RS<()> {
        todo!()
    }

    fn commit_tx(&self) -> RS<()> {
        todo!()
    }

    fn query(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)> {
        todo!()
    }

    fn command(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<usize> {
        todo!()
    }
}

unsafe impl Send for LSConn {}

unsafe impl Sync for LSConn {}