use crate::contract::cmd_exec::CmdExec;
use crate::x_engine::api::XContract;
use crate::x_engine::thd_ctx::ThdCtx;
use crate::x_engine::x_param::PDropTable;
use async_trait::async_trait;
use mudu::common::result::RS;

pub struct DropTable {
    drop_param: PDropTable,
    ctx: ThdCtx,
}

impl DropTable {
    pub fn new(drop_param: PDropTable, ctx: ThdCtx) -> Self {
        Self { drop_param, ctx }
    }
}

#[async_trait]
impl CmdExec for DropTable {
    async fn prepare(&self) -> RS<()> {
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        let xid = self.drop_param.xid;
        if let Some(t) = &self.drop_param.oid {
            self.ctx.drop_table(xid, *t).await?;
        }
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}
