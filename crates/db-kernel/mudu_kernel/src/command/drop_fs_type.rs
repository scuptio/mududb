use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::x_engine::x_param::PDropType;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use std::sync::Arc;

pub struct DropFsType {
    param: PDropType,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl DropFsType {
    pub fn new(param: PDropType, meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self { param, meta_mgr }
    }
}

#[async_trait]
impl CmdExec for DropFsType {
    async fn prepare(&self) -> RS<()> {
        if self
            .meta_mgr
            .get_fs_type_by_name(&self.param.name)
            .await?
            .is_none()
        {
            return Err(mudu_error!(
                ER::EntityNotFound,
                format!("no such filesystem type {}", self.param.name)
            ));
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        self.meta_mgr.drop_fs_type(&self.param.name).await
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}
