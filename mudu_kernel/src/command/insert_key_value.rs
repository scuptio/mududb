use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::x_engine::api::{OptInsert, XContract};
use crate::x_engine::x_param::PInsertKeyValue;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub struct InsertKeyValue {
    param: PInsertKeyValue,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
    affected_rows: AtomicU64,
}

impl InsertKeyValue {
    pub fn new(
        param: PInsertKeyValue,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            param,
            x_contract,
            meta_mgr,
            affected_rows: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl CmdExec for InsertKeyValue {
    async fn prepare(&self) -> RS<()> {
        self.prepare_inner().await
    }

    async fn run(&self) -> RS<()> {
        self.insert_inner().await
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(self.affected_rows.load(Ordering::Relaxed))
    }
}

impl InsertKeyValue {
    async fn prepare_inner(&self) -> RS<()> {
        let _ = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        for (key, _value) in &self.param.rows {
            if key.data().is_empty() {
                return Err(m_error!(ER::NoSuchElement, "key is empty"));
            }
        }
        Ok(())
    }

    async fn insert_inner(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let mut affected_rows = 0;
        for (key, value) in &self.param.rows {
            self.x_contract
                .insert(
                    self.param.tx_mgr.clone(),
                    self.param.table_id,
                    key,
                    value,
                    &OptInsert::default(),
                )
                .await?;
            affected_rows += 1;
        }
        self.affected_rows.store(affected_rows, Ordering::Relaxed);
        Ok(())
    }
}
