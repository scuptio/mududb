use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::x_engine::x_param::PCreatePartitionRule;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::sync::a_mutex::AMutex;
use std::sync::Arc;

pub struct CreatePartitionRule {
    inner: AMutex<InnerCreatePartitionRule>,
}

struct InnerCreatePartitionRule {
    param: PCreatePartitionRule,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl CreatePartitionRule {
    pub fn new(param: PCreatePartitionRule, meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self {
            inner: AMutex::new(InnerCreatePartitionRule { param, meta_mgr }),
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
            return Err(m_error!(
                ER::ExistingSuchElement,
                format!("partition rule {} already exists", self.param.rule.name)
            ));
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        self.meta_mgr.create_partition_rule(&self.param.rule).await
    }
}
