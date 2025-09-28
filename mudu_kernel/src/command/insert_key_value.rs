use crate::contract::cmd_exec::CmdExec;
use crate::sync::a_mutex::AMutex;
use crate::x_engine::api::{OptInsert, VecDatum, XContract};
use crate::x_engine::thd_ctx::ThdCtx;
use crate::x_engine::x_param::PInsertKeyValue;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::task_trace;

pub struct InsertKeyValue {
    inner: AMutex<_InsertKeyValue>,
}

struct _InsertKeyValue {
    param: PInsertKeyValue,
    thd_ctx: ThdCtx,
    affected_rows: u64,
}

impl InsertKeyValue {
    pub fn new(param: PInsertKeyValue, thd_ctx: ThdCtx) -> Self {
        Self {
            inner: AMutex::new(_InsertKeyValue::new(param, thd_ctx)),
        }
    }
}

impl _InsertKeyValue {
    fn new(param: PInsertKeyValue, thd_ctx: ThdCtx) -> Self {
        Self {
            param,
            thd_ctx,
            affected_rows: 0,
        }
    }
}

#[async_trait]
impl CmdExec for InsertKeyValue {
    async fn prepare(&self) -> RS<()> {
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        let mut g = self.inner.lock().await;
        g.insert().await?;
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        let g = self.inner.lock().await;
        Ok(g.affected_rows())
    }
}

impl _InsertKeyValue {
    async fn insert(&mut self) -> RS<()> {
        task_trace!();
        let mut key = VecDatum::default();
        let mut value = VecDatum::default();

        key.swap(&mut self.param.key);
        value.swap(&mut self.param.value);
        if key.data().is_empty() {
            return Err(m_error!(ER::NoSuchElement, "key is empty"));
        }

        self.thd_ctx
            .insert(
                self.param.xid,
                self.param.table_id,
                &self.param.key,
                &self.param.value,
                &OptInsert::default(),
            )
            .await?;
        self.affected_rows = 1;
        Ok(())
    }

    fn affected_rows(&self) -> u64 {
        self.affected_rows
    }
}
