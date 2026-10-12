use async_trait::async_trait;
use mudu::common::result::RS;

/// Command (DML/DDL) execution contract: prepare, run, then report the
/// affected-row count.
#[async_trait]
pub trait CmdExec: Send + Sync {
    /// Prepares the command for execution.
    async fn prepare(&self) -> RS<()>;
    /// Executes the prepared command.
    async fn run(&self) -> RS<()>;
    /// Returns the number of rows affected by the execution.
    async fn affected_rows(&self) -> RS<u64>;
}
