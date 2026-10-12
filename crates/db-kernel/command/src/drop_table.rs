use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::api::XContract;
use ::xengine::x_param::PDropTable;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

/// Executor of a drop-table command.
pub struct DropTable {
    drop_param: PDropTable,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl DropTable {
    /// Creates an executor for the drop-table command `drop_param`, dropping
    /// table storage through `x_contract` and reading the catalog through
    /// `meta_mgr`.
    pub fn new(
        drop_param: PDropTable,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            drop_param,
            x_contract,
            meta_mgr,
        }
    }
}

#[async_trait]
impl CmdExec for DropTable {
    async fn prepare(&self) -> RS<()> {
        if let Some(table_id) = self.drop_param.oid {
            let _ = self.meta_mgr.get_table_by_id(table_id).await?;
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        if let Some(table_id) = self.drop_param.oid {
            self.x_contract
                .drop_table(self.drop_param.tx_mgr.clone(), table_id)
                .await?;
        }
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}
