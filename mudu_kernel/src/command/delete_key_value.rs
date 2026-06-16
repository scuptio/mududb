use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::x_engine::api::{OptDelete, Predicate, XContract};
use crate::x_engine::x_param::PDeleteKeyValue;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_sys::sync::futures_mutex::FMutex;
use mudu_utils::task_trace;
use std::sync::Arc;

pub struct DeleteKeyValue {
    inner: FMutex<_DeleteKeyValue>,
}

struct _DeleteKeyValue {
    param: PDeleteKeyValue,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
    affected_rows: u64,
}

impl DeleteKeyValue {
    pub fn new(
        param: PDeleteKeyValue,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            inner: FMutex::new(_DeleteKeyValue::new(param, x_contract, meta_mgr)),
        }
    }
}

impl _DeleteKeyValue {
    fn new(
        param: PDeleteKeyValue,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            param,
            x_contract,
            meta_mgr,
            affected_rows: 0,
        }
    }

    async fn prepare(&self) -> RS<()> {
        let _ = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        if self.param.key.data().is_empty() {
            return Err(m_error!(ER::NoSuchElement, "delete key is empty"));
        }
        Ok(())
    }

    async fn run(&mut self) -> RS<()> {
        // Delete currently stays on the exact-key path to keep semantics explicit.
        let deleted = self
            .x_contract
            .delete(
                self.param.tx_mgr.clone(),
                self.param.table_id,
                &self.param.key,
                &Predicate::CNF(Vec::new()),
                &OptDelete::default(),
            )
            .await?;
        self.affected_rows = deleted as u64;
        Ok(())
    }

    fn affected_rows(&self) -> u64 {
        self.affected_rows
    }
}

#[async_trait]
impl CmdExec for DeleteKeyValue {
    async fn prepare(&self) -> RS<()> {
        let trace = task_trace!();
        trace.watch("cmd.kind", "delete");
        trace.watch("cmd.stage", "prepare_lock");
        let inner = self.inner.lock().await;
        trace.watch("cmd.stage", "prepare_inner");
        inner.prepare().await
    }

    async fn run(&self) -> RS<()> {
        let trace = task_trace!();
        trace.watch("cmd.kind", "delete");
        trace.watch("cmd.stage", "run_lock");
        let mut inner = self.inner.lock().await;
        trace.watch("cmd.stage", "run_inner");
        inner.run().await
    }

    async fn affected_rows(&self) -> RS<u64> {
        let trace = task_trace!();
        trace.watch("cmd.kind", "delete");
        trace.watch("cmd.stage", "affected_rows_lock");
        let inner = self.inner.lock().await;
        trace.watch("cmd.stage", "affected_rows_done");
        Ok(inner.affected_rows())
    }
}
