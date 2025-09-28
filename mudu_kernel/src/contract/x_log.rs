use crate::contract::lsn::LSN;
use crate::contract::waiter::Waiter;
use crate::contract::xl_rec::XLRec;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

pub struct OptAppend {
    pub wait: bool,
}

#[async_trait]
pub trait XLog: Send + Sync {
    async fn append(
        &self,
        log_rec: Vec<XLRec>,
        opt: OptAppend,
    ) -> RS<(LSN, Option<Arc<dyn Waiter<LSN>>>)>;

    async fn flush(&self, lsn: LSN) -> RS<Arc<dyn Waiter<LSN>>>;

    async fn flush_all(&self) -> RS<Arc<dyn Waiter<LSN>>>;
}

impl Default for OptAppend {
    fn default() -> Self {
        Self { wait: false }
    }
}
