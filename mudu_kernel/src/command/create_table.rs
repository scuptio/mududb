use crate::contract::cmd_exec::CmdExec;
use crate::sync::a_mutex::AMutex;
use crate::x_engine::api::XContract;
use crate::x_engine::thd_ctx::ThdCtx;
use crate::x_engine::x_param::PCreateTable;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_utils::task_trace;

pub struct CreateTable {
    inner: AMutex<_InnerCreateTable>,
}

struct _InnerCreateTable {
    param: PCreateTable,
    thd_ctx: ThdCtx,
}

impl CreateTable {
    pub fn new(param: PCreateTable, thd_ctx: ThdCtx) -> Self {
        Self {
            inner: AMutex::new(_InnerCreateTable::new(param, thd_ctx)),
        }
    }
}

#[async_trait]
impl CmdExec for CreateTable {
    async fn prepare(&self) -> RS<()> {
        task_trace!();
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        task_trace!();
        let mut g = self.inner.lock().await;
        g.run().await?;
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        task_trace!();
        Ok(0)
    }
}

impl _InnerCreateTable {
    fn new(param: PCreateTable, thd_ctx: ThdCtx) -> Self {
        Self { param, thd_ctx }
    }

    async fn run(&mut self) -> RS<()> {
        task_trace!();
        self.thd_ctx
            .create_table(self.param.xid, &self.param.schema)
            .await?;
        Ok(())
    }
}
