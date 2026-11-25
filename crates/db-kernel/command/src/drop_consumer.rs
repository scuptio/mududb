//! Executor of a `DROP CONSUMER` statement: removes a named consumer cursor
//! from a queue.
//!
//! The `_queue_consumer` row is deleted in the statement's transaction, so
//! the removal takes effect at commit and reverts on rollback. When the last
//! cursor goes the queue falls back to the default competing mode.

use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;

use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::api::XContract;
use ::xengine::x_param::PDropConsumer;

/// Executor of a drop-consumer command.
pub struct DropConsumer {
    param: PDropConsumer,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl DropConsumer {
    /// Creates an executor for the drop-consumer command `param`, removing
    /// the cursor through `x_contract` and reading the catalog through
    /// `meta_mgr`.
    pub fn new(
        param: PDropConsumer,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            param,
            x_contract,
            meta_mgr,
        }
    }
}

#[async_trait]
impl CmdExec for DropConsumer {
    async fn prepare(&self) -> RS<()> {
        // The binder resolved the queue by name; re-validate that the object
        // still exists and still is a queue.
        let desc = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        if !desc.is_queue() {
            return Err(mudu_error!(
                ER::ObjectNotFound,
                format!("no such queue {}", self.param.table_id)
            ));
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        // Re-resolve the queue so a drop between prepare and run fails here.
        self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        // Deletes the `_queue_consumer` rows (every shard) in the statement's
        // transaction; an unknown cursor fails with ObjectNotFound.
        self.x_contract
            .consumer_drop(
                self.param.tx_mgr.clone(),
                self.param.table_id,
                &self.param.consumer,
            )
            .await?;
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}
