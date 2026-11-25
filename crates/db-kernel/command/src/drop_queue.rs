use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::resolve_queue_dlq_for_drop;
use ::xengine::api::XContract;
use ::xengine::x_param::PDropQueue;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use std::sync::Arc;

/// Executor of a drop-queue command.
pub struct DropQueue {
    drop_param: PDropQueue,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl DropQueue {
    /// Creates an executor for the drop-queue command `drop_param`, dropping
    /// queue storage through `x_contract` and reading the catalog through
    /// `meta_mgr`.
    pub fn new(
        drop_param: PDropQueue,
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
impl CmdExec for DropQueue {
    async fn prepare(&self) -> RS<()> {
        // Confirms the object still exists and still is a queue; dropping a
        // plain table through DROP QUEUE is rejected.
        let desc = self.meta_mgr.get_table_by_id(self.drop_param.oid).await?;
        if !desc.is_queue() {
            return Err(mudu_error!(
                ER::ObjectNotFound,
                format!("no such queue {}", self.drop_param.oid)
            ));
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        // Cluster mode: the drop (queue + derived DLQ) travels the MetaGroup.
        if let Some(meta_group) = self.x_contract.meta_group() {
            return meta_group
                .apply(::catalog::contract::meta_op::MetaOp::DropQueue {
                    oid: self.drop_param.oid,
                })
                .await;
        }
        // The derived dead-letter queue `<name>_dlq` goes with its queue when
        // it still exists (it may have been dropped on its own).
        let desc = self.meta_mgr.get_table_by_id(self.drop_param.oid).await?;
        let dlq = resolve_queue_dlq_for_drop(self.meta_mgr.as_ref(), desc.as_ref()).await?;
        if let Some(dlq) = dlq {
            self.x_contract
                .drop_queue(self.drop_param.tx_mgr.clone(), dlq.id())
                .await?;
        } else {
            let dlq_id = catalog::contract::derive_oid::derive_table_oid(
                desc.schema(),
                &catalog::contract::queue::queue_dlq_name(desc.name()),
            );
            if self
                .meta_mgr
                .get_table_partition_binding(dlq_id)
                .await?
                .is_some()
            {
                self.meta_mgr
                    .remove_orphan_partition_binding(dlq_id)
                    .await?;
            }
        }
        self.x_contract
            .drop_queue(self.drop_param.tx_mgr.clone(), self.drop_param.oid)
            .await?;
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}
