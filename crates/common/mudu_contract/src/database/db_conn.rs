//! Synchronous and asynchronous database connection traits.

use crate::database::prepared_stmt::PreparedStmt;
use crate::database::result_set::{ResultSet, ResultSetAsync};
use crate::database::sql_params::SQLParams;
use crate::database::sql_stmt::SQLStmt;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use std::any::Any;
use std::sync::Arc;

/// A synchronous database connection.
pub trait DBConnSync: Sync + Send + Any {
    /// Executes an SQL statement discarding any result.
    fn exec_silent(&self, sql_text: &str) -> RS<()>;

    /// Begins a transaction and returns its transaction id.
    fn begin_tx(&self) -> RS<OID>;

    /// Rolls back the current transaction.
    fn rollback_tx(&self) -> RS<()>;

    /// Commits the current transaction.
    fn commit_tx(&self) -> RS<()>;

    /// Runs a query and returns the result set and row description.
    fn query(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>;

    /// Runs a command and returns the number of affected rows.
    fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64>;

    /// Runs a batch of statements and returns the number of affected rows.
    fn batch(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64>;
}

/// An asynchronous database connection.
#[async_trait]
pub trait DBConnAsync: Sync + Send + Any {
    /// Prepares an SQL statement for repeated execution.
    async fn prepare(&self, stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>>;

    /// Executes an SQL statement discarding any result.
    async fn exec_silent(&self, sql_text: String) -> RS<()>;

    /// Begins a transaction and returns its transaction id.
    async fn begin_tx(&self) -> RS<OID>;

    /// Rolls back the current transaction.
    async fn rollback_tx(&self) -> RS<()>;

    /// Commits the current transaction.
    async fn commit_tx(&self) -> RS<()>;

    /// Runs a query and returns the async result set.
    async fn query(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>>;

    /// Runs a command and returns the number of affected rows.
    async fn execute(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64>;

    /// Runs a batch of statements and returns the number of affected rows.
    async fn batch(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64>;
}
