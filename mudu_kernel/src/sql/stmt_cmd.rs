use crate::contract::cmd_exec::CmdExec;
use crate::contract::ssn_ctx::SsnCtx;
use mudu::common::result::RS;

use async_trait::async_trait;
use std::sync::Arc;

/// Async trait for converting command(Create/Alter Table, Insert/Update/Delete) statement AST node
/// to executable commands
#[async_trait]
pub trait StmtCmd {
    /// Validate and prepare the statement
    /// Naming would be converted to OID by searching metadata.
    /// Must be called before other methods.
    /// Parameters:
    /// - ctx: Session context (dynamic dispatch for implementation flexibility)
    /// Returns: Result<()> indicating success/failure
    async fn realize(&self, ctx: &dyn SsnCtx) -> RS<()>;

    /// Construct executable command
    /// Parameters:
    /// - ctx: Session context containing runtime information
    /// Returns: Thread-safe command executor wrapped in Arc
    async fn build(&self, ctx: &dyn SsnCtx) -> RS<Arc<dyn CmdExec>>;
}
