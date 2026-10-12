//! Session context registry and top-level query/command helpers.

use crate::database::db_conn::{DBConnAsync, DBConnSync};
use crate::database::entity::Entity;
use crate::database::entity_set::EntitySet;
use crate::database::result_set::{ResultSet, ResultSetAsync};
use crate::database::sql_params::SQLParams;
use crate::database::sql_stmt::SQLStmt;
use crate::database::v2h_param::QueryResult;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_type::datum::DatumDyn;
use scc::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::debug;

/// Passes an SQL statement trait object through unchanged.
pub fn function_sql_stmt(stmt: &dyn SQLStmt) -> &dyn SQLStmt {
    stmt
}

/// Passes an SQL parameter slice through unchanged.
pub fn function_sql_param<'a>(param: &'a [&'a dyn DatumDyn]) -> &'a [&'a dyn DatumDyn] {
    param
}

lazy_static! {
    static ref SessionContext: HashMap<OID, Context> = HashMap::new();
}

/// A database connection, either synchronous or asynchronous.
#[derive(Clone)]
pub enum DBConn {
    /// A synchronous connection.
    Sync(Arc<dyn DBConnSync>),
    /// An asynchronous connection.
    Async(Arc<dyn DBConnAsync>),
}

impl DBConn {
    /// Begins a transaction and returns its transaction id.
    pub async fn begin_tx(&self) -> RS<OID> {
        let xid = match self {
            DBConn::Sync(conn) => conn.begin_tx()?,
            DBConn::Async(conn) => conn.begin_tx().await?,
        };
        Ok(xid)
    }

    /// Executes an SQL statement discarding any result.
    pub async fn execute_silent(&self, sql: String) -> RS<()> {
        match self {
            DBConn::Sync(conn) => conn.exec_silent(&sql),
            DBConn::Async(conn) => conn.exec_silent(sql).await,
        }
    }

    /// Returns the synchronous connection; panics if this is an async connection.
    pub fn expected_sync(&self) -> RS<&dyn DBConnSync> {
        match self {
            DBConn::Sync(s) => Ok(s.as_ref()),
            DBConn::Async(_) => unreachable!(),
        }
    }

    /// Returns the asynchronous connection; panics if this is a sync connection.
    pub fn expected_async(&self) -> RS<&dyn DBConnAsync> {
        match self {
            DBConn::Sync(_) => unreachable!(),
            DBConn::Async(s) => Ok(s.as_ref()),
        }
    }
}

/// A session context binding a connection, the current transaction, and the
/// latest cached result set.
#[derive(Clone)]
pub struct Context {
    inner: Arc<ContextInner>,
}

struct ContextInner {
    failed: AtomicBool,
    session_id: OID,
    xid: SMutex<OID>,
    result_set: SMutex<Option<ContextResult>>,
    conn: DBConn,
    /// Application whose schema namespace this context's unqualified names
    /// resolve against (`""` maps to the default schema server-side).
    app_name: String,
}

struct ContextResult {
    result_set: Arc<dyn ResultSet>,
    row_desc: Arc<TupleFieldDesc>,
}

impl ContextResult {
    fn new(result_set: Arc<dyn ResultSet>, row_desc: Arc<TupleFieldDesc>) -> RS<Self> {
        Ok(Self {
            result_set,
            row_desc,
        })
    }

    fn row_desc(&self) -> &TupleFieldDesc {
        &self.row_desc
    }

    fn query_next(&self) -> RS<Option<TupleValue>> {
        let row = self.result_set.next()?;
        Ok(row)
    }
}

impl ContextInner {
    fn new(oid: OID, conn: DBConn, app_name: String) -> RS<Self> {
        let s = Self {
            failed: AtomicBool::new(false),
            session_id: oid,
            xid: SMutex::new(0),
            result_set: SMutex::new(Default::default()),
            conn,
            app_name,
        };
        Ok(s)
    }

    fn set_xid(&self, xid: OID) {
        if let Ok(mut guard) = self.xid.lock() {
            *guard = xid;
        }
    }
    fn xid(&self) -> OID {
        self.xid.lock().map(|g| *g).unwrap_or(0)
    }
    fn session_id(&self) -> OID {
        self.session_id
    }
    fn query<R: Entity>(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<EntitySet<R>> {
        let (rs, rd) = self.conn.expected_sync()?.query(sql, param)?;
        Ok(EntitySet::<R>::new(rs, rd))
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

    fn batch(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.conn.expected_sync()?.batch(sql, param)
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

    async fn batch_async(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        self.conn.expected_async()?.batch(sql, param).await
    }

    fn cache_result(&self, result: (Arc<dyn ResultSet>, Arc<TupleFieldDesc>)) -> RS<QueryResult> {
        let mut g = self
            .result_set
            .lock()
            .map_err(|e| mudu_error!(ErrorCode::Mutex, "result_set lock poisoned", e))?;
        let context_result = ContextResult::new(result.0, result.1)?;

        let result = QueryResult::new(self.session_id, context_result.row_desc().clone());
        *g = Some(context_result);
        Ok(result)
    }

    pub fn query_next(&self) -> RS<Option<TupleValue>> {
        let mut g = self
            .result_set
            .lock()
            .map_err(|e| mudu_error!(ErrorCode::Mutex, "result_set lock poisoned", e))?;
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
    /// Creates and registers a context for session `oid` on `conn`.
    pub fn create(oid: OID, conn: DBConn) -> RS<Context> {
        Context::create_with_app(oid, conn, String::new())
    }

    /// Create a context bound to an application schema (the application name
    /// from `package.cfg.json`; `""` maps to the default schema server-side).
    pub fn create_with_app(oid: OID, conn: DBConn, app_name: String) -> RS<Context> {
        Context::new(oid, conn, app_name)
    }

    /// Marks an externally owned transaction unusable until rollback succeeds.
    pub fn mark_failed(&self) {
        self.inner.failed.store(true, Ordering::Release);
    }

    /// Rejects SQL and commit after a failed procedure in this transaction.
    pub fn ensure_usable(&self) -> RS<()> {
        if self.inner.failed.load(Ordering::Acquire) {
            return Err(mudu_error!(
                ErrorCode::Transaction,
                "transaction requires rollback after failed procedure"
            ));
        }
        Ok(())
    }

    /// Begins a transaction on the context's connection and records its id.
    pub async fn begin_tx(&self) -> RS<()> {
        self.ensure_usable()?;
        let xid = self.inner.conn.begin_tx().await?;
        self.inner.set_xid(xid);
        debug!("transaction begin {}", xid);
        Ok(())
    }

    /// Looks up a registered context by session id.
    #[allow(clippy::self_named_constructors)]
    pub fn context(oid: OID) -> Option<Context> {
        let opt = SessionContext.get_sync(&oid);
        opt.map(|e| e.get().clone())
    }

    /// Removes a registered context by session id.
    pub fn remove(xid: OID) -> Option<Context> {
        let opt = SessionContext.remove_sync(&xid);
        opt.map(|e| e.1)
    }

    /// Removes a registered context by session id, asynchronously.
    pub async fn remove_async(xid: OID) -> Option<Context> {
        let opt = SessionContext.remove_async(&xid).await;
        opt.map(|e| e.1)
    }

    /// Commits the transaction of the context registered under `xid`, if any.
    pub fn commit(xid: OID) -> RS<()> {
        let opt = SessionContext.get_sync(&xid);
        match opt {
            Some(e) => e.get().commit_tx(),
            None => Ok(()),
        }
    }

    /// Rolls back the transaction of the context registered under `xid`, if any.
    pub fn rollback(xid: OID) -> RS<()> {
        let opt = SessionContext.get_sync(&xid);
        match opt {
            Some(e) => e.get().rollback_tx(),
            None => Ok(()),
        }
    }

    /// Commits the transaction of the context registered under `oid`, asynchronously.
    pub async fn commit_async(oid: OID) -> RS<()> {
        let ctx = Self::context_async(oid).await?;
        ctx.commit_tx_async().await?;
        let xid = ctx.inner.xid();
        debug!("transaction committed {}", xid);
        Ok(())
    }

    /// Rolls back the transaction of the context registered under `oid`, asynchronously.
    pub async fn rollback_async(oid: OID) -> RS<()> {
        let ctx = Self::context_async(oid).await?;
        ctx.rollback_tx_async().await?;
        let xid = ctx.inner.xid();
        debug!("transaction rollback {}", xid);
        Ok(())
    }

    /// Looks up a registered context by session id, asynchronously.
    pub async fn context_async(xid: OID) -> RS<Context> {
        let ctx = {
            let opt = SessionContext.get_async(&xid).await;
            match opt {
                Some(e) => e.get().clone(),
                None => return Err(mudu_error!(ErrorCode::EntityNotFound, "no such context")),
            }
        };
        Ok(ctx)
    }

    /// Returns the session id of this context.
    pub fn session_id(&self) -> OID {
        self.inner.session_id()
    }

    /// The database connection this context executes statements on.
    pub fn db_conn(&self) -> &DBConn {
        &self.inner.conn
    }

    /// The application whose schema namespace this context resolves against.
    pub fn app_name(&self) -> &str {
        &self.inner.app_name
    }
    fn rollback_tx(&self) -> RS<()> {
        self.inner.conn.expected_sync()?.rollback_tx()?;
        self.inner.failed.store(false, Ordering::Release);
        Ok(())
    }

    fn commit_tx(&self) -> RS<()> {
        self.ensure_usable()?;
        self.inner.conn.expected_sync()?.commit_tx()
    }

    async fn rollback_tx_async(&self) -> RS<()> {
        self.inner.conn.expected_async()?.rollback_tx().await?;
        self.inner.failed.store(false, Ordering::Release);
        Ok(())
    }

    async fn commit_tx_async(&self) -> RS<()> {
        self.ensure_usable()?;
        self.inner.conn.expected_async()?.commit_tx().await
    }

    fn new(oid: OID, conn: DBConn, app_name: String) -> RS<Self> {
        let s = Self {
            inner: Arc::new(ContextInner::new(oid, conn, app_name)?),
        };
        let _ = SessionContext.insert_sync(s.session_id(), s.clone());
        Ok(s)
    }

    /// Runs a query and maps the result rows to entities of type `R`.
    pub fn query<R: Entity>(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<EntitySet<R>> {
        self.ensure_usable()?;
        self.inner.query(sql, param)
    }

    /// Runs a query and returns the raw result set and row description.
    pub fn query_raw(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        self.ensure_usable()?;
        self.inner.query_raw(sql, param)
    }

    /// Runs a query asynchronously and returns the raw async result set.
    pub async fn query_raw_async(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        self.ensure_usable()?;
        self.inner.query_raw_async(sql, param).await
    }

    /// Runs a command and returns the number of affected rows.
    pub fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.ensure_usable()?;
        self.inner.command(sql, param)
    }

    /// Runs a command asynchronously and returns the number of affected rows.
    pub async fn command_async(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        self.ensure_usable()?;
        self.inner.command_async(sql, param).await
    }

    /// Runs a batch of statements and returns the number of affected rows.
    pub fn batch(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.ensure_usable()?;
        self.inner.batch(sql, param)
    }

    /// Runs a batch of statements asynchronously and returns the number of affected rows.
    pub async fn batch_async(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        self.ensure_usable()?;
        self.inner.batch_async(sql, param).await
    }

    // for naive implementation
    /// Caches a query result in this context and returns its cursor descriptor.
    pub fn cache_result(
        &self,
        result: (Arc<dyn ResultSet>, Arc<TupleFieldDesc>),
    ) -> RS<QueryResult> {
        self.inner.cache_result(result)
    }

    /// Fetches the next row of the cached result set, if any.
    pub fn query_next(&self) -> RS<Option<TupleValue>> {
        self.ensure_usable()?;
        self.inner.query_next()
    }
}

/// Runs a query on the context registered under `xid`.
pub fn mudu_query<R: Entity>(
    xid: OID,
    sql: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<EntitySet<R>> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("mudu_query, no such transaction {}", xid))?;
    context.query(sql, param)
}

/// Runs a command on the context registered under `xid`.
pub fn mudu_command(xid: OID, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("mudu_command, no such transaction {}", xid))?;
    context.command(sql, param)
}

/// Runs a batch of statements on the context registered under `xid`.
pub fn mudu_batch(xid: OID, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
    let r = Context::context(xid);
    let context = rs_option(r, &format!("mudu_batch, no such transaction {}", xid))?;
    context.batch(sql, param)
}

#[cfg(test)]
#[path = "sql_invocation_test.rs"]
mod invocation_test;
