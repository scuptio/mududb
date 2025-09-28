use crate::contract::query_exec::QueryExec;
use crate::contract::ssn_ctx::SsnCtx;
use crate::sql::proj_list::ProjList;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

/// Define async trait for SELECT query statements
#[async_trait]
pub trait StmtQuery {
    /// Validate and prepare the statement
    /// Naming would be converted to OID by searching metadata.
    /// Must be called before other methods.
    /// # Arguments
    /// * `ctx` - Reference to session context implementing SsnCtx
    async fn realize(&self, ctx: &dyn SsnCtx) -> RS<()>;

    /// Constructs an executable query object
    /// # Arguments
    /// * `ctx` - Reference to session context implementing SsnCtx
    /// # Returns
    /// Thread-safe reference-counted pointer to QueryExec implementation
    async fn build(&self, ctx: &dyn SsnCtx) -> RS<Arc<dyn QueryExec>>;

    /// Retrieves the projection list(SELECT term) for the query
    /// # Returns
    /// Result containing the projection list structure
    fn proj_list(&self) -> RS<ProjList>;
}
