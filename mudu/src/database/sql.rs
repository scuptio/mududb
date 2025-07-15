use crate::common::result::RS;
use crate::common::result_of::rs_option;
use crate::common::xid::XID;
use crate::database::db_conn::DBConn;
use crate::database::record::Record;
use crate::database::record_set::RecordSet;
use crate::database::sql_stmt::SQLStmt;
use crate::tuple::to_datum::ToDatum;
use lazy_static::lazy_static;
use scc::HashMap;
use std::sync::Arc;

pub fn function_sql_stmt(stmt:&dyn SQLStmt) -> &dyn SQLStmt {
    stmt
}

pub fn function_sql_param<'a>(param:&'a  [&'a dyn ToDatum]) -> &'a  [&'a  dyn ToDatum] {
    param
}

lazy_static! {
    static ref XContext:HashMap<XID, Context> = HashMap::new();
}

#[derive(Clone)]
pub struct Context {
    xid: XID,
    context:Arc<dyn DBConn>,
}


impl Context {
    pub fn create(conn:Arc<dyn DBConn>) -> RS<Context> {
        Context::new(conn)
    }

    pub fn context(xid: XID) -> Option<Context> {
        let opt = XContext.get(&xid);
        opt.map(|e| { e.get().clone() } )
    }

    pub fn remove(xid: XID) -> Option<Context> {
        let opt = XContext.remove(&xid);
        opt.map(|e| { e.1 })
    }

    pub fn xid(&self) -> XID {
        self.xid
    }

    fn new(conn:Arc<dyn DBConn>) -> RS<Self> {
        let xid = conn.begin_tx()?;
        let s = Self {
            xid,
            context:conn,
        };
        let _ = XContext.insert(xid, s.clone());
        Ok(s)
    }

    pub fn query<R:Record>(
        &self,
        sql:&dyn SQLStmt,
        param:&[&dyn ToDatum]
    ) -> RS<RecordSet<R>> {
        let (rs, rd) = self.context.query(sql, param)?;
        Ok(RecordSet::<R>::new(rs, rd))
    }

    pub fn command(
        &self,
        sql:&dyn SQLStmt,
        param:&[&dyn ToDatum]
    ) -> RS<usize> {
        self.context.command(sql, param)
    }
}



pub fn context(
    conn:Arc<dyn DBConn>,
) -> RS<Context> {
    Context::create(conn)
}

pub fn query<R:Record>(
    xid: XID,
    sql:&dyn SQLStmt,
    param:&[&dyn ToDatum]
) -> RS<RecordSet<R>> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("no such transaction {}", xid))?;
    context.query(sql, param)
}

pub fn command(
    xid: XID,
    sql:&dyn SQLStmt,
    param:&[&dyn ToDatum]
) -> RS<usize> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("no such transaction {}", xid))?;
    context.command(sql, param)
}


