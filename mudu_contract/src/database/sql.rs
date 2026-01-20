use crate::database::db_conn::{DBConnAsync, DBConnSync};
use crate::database::entity::Entity;
use crate::database::entity_set::RecordSet;
use crate::database::result_set::{ResultSet, ResultSetAsync};
use crate::database::sql_params::SQLParams;
use crate::database::sql_stmt::SQLStmt;
use crate::database::v2h_param::QueryResult;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::common::xid::XID;
use mudu_type::datum::DatumDyn;
use scc::HashMap;
use std::sync::{Arc, Mutex};

pub fn function_sql_stmt(stmt: &dyn SQLStmt) -> &dyn SQLStmt {
    stmt
}

pub fn function_sql_param<'a>(param: &'a [&'a dyn DatumDyn]) -> &'a [&'a dyn DatumDyn] {
    param
}

lazy_static! {
    static ref SessionContext: HashMap<OID, Context> = HashMap::new();
}

#[derive(Clone)]
pub enum DBConn {
    Sync(Arc<dyn DBConnSync>),
    Async(Arc<dyn DBConnAsync>),
}

impl DBConn {
    pub async fn begin_tx(&self) -> RS<()> {
        match self {
            DBConn::Sync(conn) => {
                let _ = conn.begin_tx()?;
            }
            DBConn::Async(conn) => {
                let _ = conn.begin_tx().await?;
            }
        }
        Ok(())
    }

    pub async fn execute_silent(&self, sql: String) -> RS<()> {
        match self {
            DBConn::Sync(conn) => {
                conn.exec_silent(&sql)
            }
            DBConn::Async(conn) => {
                conn.exec_silent(sql).await
            }
        }
    }

    pub fn expected_sync(&self) -> RS<&dyn DBConnSync> {
        match self {
            DBConn::Sync(s) => { Ok(s.as_ref()) }
            DBConn::Async(_) => {
                unsafe { std::hint::unreachable_unchecked() }
            }
        }
    }

    pub fn expected_async(&self) -> RS<&dyn DBConnAsync> {
        match self {
            DBConn::Sync(_) => {
                unsafe { std::hint::unreachable_unchecked() }
            }
            DBConn::Async(s) => { Ok(s.as_ref()) }
        }
    }
}
#[derive(Clone)]
pub struct Context {
    inner: Arc<ContextInner>,
}

struct ContextInner {
    xid: XID,
    result_set: Mutex<Option<ContextResult>>,
    conn: DBConn,
}

struct ContextResult {
    result_set: Arc<dyn ResultSet>,
    row_desc: Arc<TupleFieldDesc>,
    _tuple_desc: Arc<TupleBinaryDesc>,
    datum_mapping: Vec<usize>,
}

impl ContextResult {
    fn new(result_set: Arc<dyn ResultSet>, row_desc: Arc<TupleFieldDesc>) -> RS<Self> {
        let (tuple_desc, datum_mapping) = row_desc.to_tuple_binary_desc()?;
        Ok(Self {
            result_set,
            row_desc,
            _tuple_desc: Arc::new(tuple_desc),
            datum_mapping,
        })
    }

    fn row_desc(&self) -> &TupleFieldDesc {
        &self.row_desc
    }

    #[allow(dead_code)]
    fn tuple_desc(&self) -> &TupleBinaryDesc {
        &self._tuple_desc
    }
    #[allow(dead_code)]
    fn datum_mapping(&self) -> &Vec<usize> {
        &self.datum_mapping
    }

    fn query_next(&self) -> RS<Option<TupleValue>> {
        let row = self.result_set.next()?;
        Ok(row)
    }
}

impl ContextInner {
    fn new(oid: OID, conn: DBConn) -> RS<Self> {
        let s = Self {
            xid: oid,
            result_set: Mutex::new(Default::default()),
            conn,
        };
        Ok(s)
    }

    fn xid(&self) -> XID {
        self.xid
    }
    fn query<R: Entity>(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<RecordSet<R>> {
        let (rs, rd) = self.conn.expected_sync()?.query(sql, param)?;
        Ok(RecordSet::<R>::new(rs, rd))
    }

    fn query_raw(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        self.conn.expected_sync()?.query(sql, param)
    }

    fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.conn.expected_sync()?.command(sql, param)
    }

    async fn query_raw_async(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        self.conn.expected_async()?.query(sql, param).await
    }

    async fn command_async(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        self.conn.expected_async()?.execute(sql, param).await
    }

    fn cache_result(&self, result: (Arc<dyn ResultSet>, Arc<TupleFieldDesc>)) -> RS<QueryResult> {
        let mut g = self.result_set.lock().unwrap();
        let context_result = ContextResult::new(result.0, result.1)?;

        let result = QueryResult::new(self.xid, context_result.row_desc().clone());
        *g = Some(context_result);
        Ok(result)
    }

    pub fn query_next(&self) -> RS<Option<TupleValue>> {
        let mut g = self.result_set.lock().unwrap();
        match &*g {
            None => Ok(None),
            Some(result) => {
                let opt = result.query_next()?;
                if opt.is_none() {
                    *g = None;
                }
                Ok(opt)
            }
        }
    }
}

impl Context {
    pub fn create(oid: OID, conn: DBConn) -> RS<Context> {
        Context::new(oid, conn)
    }

    pub async fn begin_tx(&self) -> RS<()> {
        self.inner.conn.begin_tx().await
    }

    pub fn context(oid: OID) -> Option<Context> {
        let opt = SessionContext.get_sync(&oid);
        opt.map(|e| e.get().clone())
    }

    pub fn remove(xid: XID) -> Option<Context> {
        let opt = SessionContext.remove_sync(&xid);
        opt.map(|e| e.1)
    }

    pub fn commit(xid: XID) -> RS<()> {
        let opt = SessionContext.get_sync(&xid);
        match opt {
            Some(e) => e.get().commit_tx(),
            None => Ok(()),
        }
    }

    pub fn rollback(xid: XID) -> RS<()> {
        let opt = SessionContext.get_sync(&xid);
        match opt {
            Some(e) => e.get().rollback_tx(),
            None => Ok(()),
        }
    }

    pub async fn commit_async(xid: XID) -> RS<()> {
        let opt = SessionContext.get_sync(&xid);
        match opt {
            Some(e) => e.get().commit_tx_async().await,
            None => Ok(()),
        }
    }

    pub async fn rollback_async(xid: XID) -> RS<()> {
        let opt = SessionContext.get_sync(&xid);
        match opt {
            Some(e) => e.get().rollback_tx_async().await,
            None => Ok(()),
        }
    }
    pub fn session_id(&self) -> XID {
        self.inner.xid()
    }
    fn rollback_tx(&self) -> RS<()> {
        self.inner.conn.expected_sync()?.rollback_tx()
    }

    fn commit_tx(&self) -> RS<()> {
        self.inner.conn.expected_sync()?.commit_tx()
    }

    async fn rollback_tx_async(&self) -> RS<()> {
        self.inner.conn.expected_async()?.rollback_tx().await
    }

    async fn commit_tx_async(&self) -> RS<()> {
        self.inner.conn.expected_async()?.commit_tx().await
    }

    fn new(oid: OID, conn: DBConn) -> RS<Self> {
        let s = Self {
            inner: Arc::new(ContextInner::new(oid, conn)?),
        };
        let _ = SessionContext.insert_sync(s.session_id(), s.clone());
        Ok(s)
    }

    pub fn query<R: Entity>(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<RecordSet<R>> {
        self.inner.query(sql, param)
    }

    pub fn query_raw(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        self.inner.query_raw(sql, param)
    }

    pub async fn query_raw_async(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        self.inner.query_raw_async(sql, param).await
    }

    pub fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.inner.command(sql, param)
    }

    pub async fn command_async(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        self.inner.command_async(sql, param).await
    }

    // for naive implementation
    pub fn cache_result(
        &self,
        result: (Arc<dyn ResultSet>, Arc<TupleFieldDesc>),
    ) -> RS<QueryResult> {
        self.inner.cache_result(result)
    }

    pub fn query_next(&self) -> RS<Option<TupleValue>> {
        self.inner.query_next()
    }
}


pub fn mudu_query<R: Entity>(
    xid: XID,
    sql: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("mudu_query, no such transaction {}", xid))?;
    context.query(sql, param)
}

pub fn mudu_command(xid: XID, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("mudu_command, no such transaction {}", xid))?;
    context.command(sql, param)
}
