//! A lightweight context wrapping a synchronous connection.

use crate::database::db_conn::DBConnSync;
use crate::database::object::Object;
use crate::database::object_set::ObjectSet;
use crate::database::sql_params::SQLParams;
use crate::database::sql_stmt::SQLStmt;
use mudu::common::result::RS;
use std::sync::Arc;

/// A context executing statements on a synchronous connection.
pub struct Context {
    conn: Arc<dyn DBConnSync>,
}

impl Context {
    /// Creates a context on `conn`.
    pub fn new(conn: Arc<dyn DBConnSync>) -> Self {
        Self { conn }
    }

    /// Returns the connection of this context.
    pub fn db_conn(&self) -> Arc<dyn DBConnSync> {
        self.conn.clone()
    }

    /// Runs a query and maps the result rows to entities of type `R`.
    pub fn query<R: Object>(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<ObjectSet<R>> {
        let (rs, ds) = self.conn.query(sql, param)?;
        let r = ObjectSet::<R>::new(rs, ds);
        Ok(r)
    }

    /// Runs a command and returns the number of affected rows.
    pub fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.conn.command(sql, param)
    }
}
