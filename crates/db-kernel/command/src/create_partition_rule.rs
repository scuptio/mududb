use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::x_param::PCreatePartitionRule;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::sync::async_::AMutex;
use std::sync::Arc;

/// Executor of a create-partition-rule command.
pub struct CreatePartitionRule {
    inner: AMutex<InnerCreatePartitionRule>,
}

struct InnerCreatePartitionRule {
    param: PCreatePartitionRule,
    meta_mgr: Arc<dyn MetaMgr>,
    meta_group: Option<Arc<dyn ::catalog::contract::meta_op::MetaGroupApi>>,
}

impl CreatePartitionRule {
    /// Creates an executor for the create-partition-rule command `param`,
    /// reading and writing the partition-rule catalog through `meta_mgr`.
    pub fn new(
        param: PCreatePartitionRule,
        meta_mgr: Arc<dyn MetaMgr>,
        meta_group: Option<Arc<dyn ::catalog::contract::meta_op::MetaGroupApi>>,
    ) -> Self {
        Self {
            inner: AMutex::new(InnerCreatePartitionRule {
                param,
                meta_mgr,
                meta_group,
            }),
        }
    }
}

#[async_trait]
impl CmdExec for CreatePartitionRule {
    async fn prepare(&self) -> RS<()> {
        let inner = self.inner.lock().await;
        inner.prepare().await
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let inner = self.inner.lock().await;
        inner.run().await
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}

impl InnerCreatePartitionRule {
    async fn prepare(&self) -> RS<()> {
        if self
            .meta_mgr
            .get_partition_rule_by_name(&self.param.rule.name)
            .await?
            .is_some()
        {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                format!("partition rule {} already exists", self.param.rule.name)
            ));
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        if let Some(meta_group) = &self.meta_group {
            return meta_group
                .apply(::catalog::contract::meta_op::MetaOp::CreatePartitionRule {
                    rule: self.param.rule.clone(),
                })
                .await;
        }
        self.meta_mgr.create_partition_rule(&self.param.rule).await
    }
}
