use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::x_param::PCreateFsType;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use std::sync::Arc;

/// Executor of a create-filesystem-type command.
pub struct CreateFsType {
    param: PCreateFsType,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl CreateFsType {
    /// Creates an executor for the create-filesystem-type command `param`,
    /// reading the filesystem-type catalog through `meta_mgr`.
    pub fn new(param: PCreateFsType, meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self { param, meta_mgr }
    }
}

#[async_trait]
impl CmdExec for CreateFsType {
    async fn prepare(&self) -> RS<()> {
        if self
            .meta_mgr
            .get_fs_type_by_name(&self.param.name)
            .await?
            .is_some()
        {
            return Err(mudu_error!(
                ER::AlreadyExists,
                format!("filesystem type {} already exists", self.param.name)
            ));
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        self.meta_mgr
            .create_fs_type(&self.param.name, self.param.kind)
            .await?;
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}
