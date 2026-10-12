//! Prepared statement abstraction.

use crate::database::result_set::ResultSetAsync;
use crate::database::sql_params::SQLParams;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use std::sync::Arc;

/// An SQL statement prepared for repeated execution.
#[async_trait::async_trait]
pub trait PreparedStmt: Send + Sync {
    /// Runs the prepared statement as a query with `params`.
    async fn query(&self, params: Box<dyn SQLParams>) -> RS<Arc<dyn ResultSetAsync>>;

    /// Runs the prepared statement as a command with `params`, returning the affected-row count.
    async fn execute(&self, params: Box<dyn SQLParams>) -> RS<u64>;

    /// Returns the description of the result rows.
    async fn desc(&self) -> RS<Arc<TupleFieldDesc>>;

    /// Resets the prepared statement to its initial state.
    async fn reset(&self) -> RS<()>;
}
