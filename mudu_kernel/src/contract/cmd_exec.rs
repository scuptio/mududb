use async_trait::async_trait;
use mudu::common::result::RS;

#[async_trait]
pub trait CmdExec: Send + Sync {
    async fn prepare(&self) -> RS<()>;
    async fn run(&self) -> RS<()>;
    async fn affected_rows(&self) -> RS<u64>;
}
