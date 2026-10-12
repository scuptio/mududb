use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::x_param::PCreatePartitionPlacement;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_sys::sync::async_::AMutex;
use std::sync::Arc;

/// Executor of a create-partition-placement command.
pub struct CreatePartitionPlacement {
    inner: AMutex<InnerCreatePartitionPlacement>,
}

struct InnerCreatePartitionPlacement {
    param: PCreatePartitionPlacement,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl CreatePartitionPlacement {
    /// Creates an executor for the create-partition-placement command
    /// `param`, writing placements through `meta_mgr`.
    pub fn new(param: PCreatePartitionPlacement, meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self {
            inner: AMutex::new(InnerCreatePartitionPlacement { param, meta_mgr }),
        }
    }
}

#[async_trait]
impl CmdExec for CreatePartitionPlacement {
    async fn prepare(&self) -> RS<()> {
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let inner = self.inner.lock().await;
        inner
            .meta_mgr
            .upsert_partition_placements(&inner.param.placements)
            .await
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}
