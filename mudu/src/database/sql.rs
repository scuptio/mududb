use crate::common::result::RS;
use crate::common::result_of::rs_option;
use crate::common::xid::XID;
use crate::database::db_conn::DBConn;
use crate::database::record::Record;
use crate::database::record_set::RecordSet;
use crate::database::result_set::ResultSet;
use crate::database::sql_stmt::SQLStmt;
use crate::database::v2h_param::QueryResult;
use crate::tuple::datum::DatumDyn;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use crate::tuple::tuple_item::TupleItem;
use crate::tuple::tuple_item_desc::TupleItemDesc;
use lazy_static::lazy_static;
use scc::HashMap;
use std::sync::{Arc, Mutex};

pub fn function_sql_stmt(stmt: &dyn SQLStmt) -> &dyn SQLStmt {
    stmt
}

pub fn function_sql_param<'a>(param: &'a [&'a dyn DatumDyn]) -> &'a [&'a dyn DatumDyn] {
    param
}

lazy_static! {
    static ref XContext:HashMap<XID, Context> = HashMap::new();
}

#[derive(Clone)]
pub struct Context {
    inner: Arc<ContextInner>,
}

struct ContextInner {
    xid: XID,
    result_set: Mutex<Option<ContextResult>>,
    context: Arc<dyn DBConn>,
}

struct ContextResult {
    result_set: Arc<dyn ResultSet>,
    row_desc: Arc<TupleItemDesc>,
    tuple_desc: Arc<TupleBinaryDesc>,
    datum_mapping: Vec<usize>,
}

impl ContextResult {
    fn new(
        result_set: Arc<dyn ResultSet>,
        row_desc: Arc<TupleItemDesc>,
    ) -> Self {
        let (tuple_desc, datum_mapping) = row_desc.to_tuple_desc();
        Self {
            result_set,
            row_desc,
            tuple_desc: Arc::new(tuple_desc),
            datum_mapping,
        }
    }

    fn row_desc(&self) -> &TupleItemDesc {
        &self.row_desc
    }

    fn tuple_desc(&self) -> &TupleBinaryDesc {
        &self.tuple_desc
    }

    fn datum_mapping(&self) -> &Vec<usize> {
        &self.datum_mapping
    }

    fn query_next(&self) -> RS<Option<TupleItem>> {
        let row = self.result_set.next()?;
        Ok(row)
    }
}

impl ContextInner {
    fn new(conn: Arc<dyn DBConn>) -> RS<Self> {
        let xid = conn.begin_tx()?;
        let s = Self {
            xid,
            result_set: Mutex::new(Default::default()),
            context: conn,
        };
        Ok(s)
    }

    fn xid(&self) -> XID {
        self.xid
    }
    fn query<R: Record>(
        &self,
        sql: &dyn SQLStmt,
        param: &[&dyn DatumDyn],
    ) -> RS<RecordSet<R>> {
        let (rs, rd) = self.context.query(sql, param)?;
        Ok(RecordSet::<R>::new(rs, rd))
    }

    fn query_raw(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)> {
        self.context.query(sql, param)
    }

    fn command(
        &self,
        sql: &dyn SQLStmt,
        param: &[&dyn DatumDyn],
    ) -> RS<usize> {
        self.context.command(sql, param)
    }

    fn cache_result(
        &self,
        result: (Arc<dyn ResultSet>, Arc<TupleItemDesc>),
    ) -> RS<QueryResult> {
        let mut g = self.result_set.lock().unwrap();
        let context_result = ContextResult::new(result.0, result.1);

        let result = QueryResult::new(
            self.xid,
            context_result.row_desc().clone(),
        );
        *g = Some(context_result);
        Ok(result)
    }

    pub fn query_next(
        &self,
    ) -> RS<Option<TupleItem>> {
        let mut g = self.result_set.lock().unwrap();
        match &*g {
            None => {
                Ok(None)
            }
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
    pub fn create(conn: Arc<dyn DBConn>) -> RS<Context> {
        Context::new(conn)
    }

    pub fn context(xid: XID) -> Option<Context> {
        let opt = XContext.get_sync(&xid);
        opt.map(|e| { e.get().clone() })
    }

    pub fn remove(xid: XID) -> Option<Context> {
        let opt = XContext.remove_sync(&xid);
        opt.map(|e| { e.1 })
    }

    pub fn xid(&self) -> XID {
        self.inner.xid()
    }

    pub fn new(conn: Arc<dyn DBConn>) -> RS<Self> {
        let xid = conn.begin_tx()?;
        let s = Self {
            inner: Arc::new(ContextInner::new(conn)?),
        };
        let _ = XContext.insert_sync(xid, s.clone());
        Ok(s)
    }

    pub fn query<R: Record>(
        &self,
        sql: &dyn SQLStmt,
        param: &[&dyn DatumDyn],
    ) -> RS<RecordSet<R>> {
        self.inner.query(sql, param)
    }

    pub fn query_raw(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)> {
        self.inner.query_raw(sql, param)
    }

    pub fn command(
        &self,
        sql: &dyn SQLStmt,
        param: &[&dyn DatumDyn],
    ) -> RS<usize> {
        self.inner.command(sql, param)
    }


    // for naive implementation
    pub fn cache_result(
        &self,
        result: (Arc<dyn ResultSet>, Arc<TupleItemDesc>),
    ) -> RS<QueryResult> {
        self.inner.cache_result(result)
    }

    pub fn query_next(
        &self,
    ) -> RS<Option<TupleItem>> {
        self.inner.query_next()
    }
}


pub fn context(
    conn: Arc<dyn DBConn>,
) -> RS<Context> {
    Context::create(conn)
}

pub fn query<R: Record>(
    xid: XID,
    sql: &dyn SQLStmt,
    param: &[&dyn DatumDyn],
) -> RS<RecordSet<R>> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("no such transaction {}", xid))?;
    context.query(sql, param)
}

pub fn command(
    xid: XID,
    sql: &dyn SQLStmt,
    param: &[&dyn DatumDyn],
) -> RS<usize> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("no such transaction {}", xid))?;
    context.command(sql, param)
}


