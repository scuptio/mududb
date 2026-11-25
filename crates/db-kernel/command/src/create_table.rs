use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::api::XContract;
use ::xengine::x_param::PCreateTable;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::sync::async_::AMutex;
use std::sync::Arc;
use tracing::trace;

/// Executor of a create-table command.
pub struct CreateTable {
    inner: AMutex<_InnerCreateTable>,
}

struct _InnerCreateTable {
    param: PCreateTable,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl CreateTable {
    /// Creates an executor for the create-table command `param`, creating
    /// table storage through `x_contract` and reading and writing the catalog
    /// through `meta_mgr`.
    pub fn new(
        param: PCreateTable,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            inner: AMutex::new(_InnerCreateTable::new(param, x_contract, meta_mgr)),
        }
    }
}

#[async_trait]
impl CmdExec for CreateTable {
    async fn prepare(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let inner = self.inner.lock().await;
        inner.prepare().await
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let mut inner = self.inner.lock().await;
        inner.run().await
    }

    async fn affected_rows(&self) -> RS<u64> {
        mudu_utils::scoped_task_trace!();
        Ok(0)
    }
}

impl _InnerCreateTable {
    fn new(
        param: PCreateTable,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            param,
            x_contract,
            meta_mgr,
        }
    }

    async fn prepare(&self) -> RS<()> {
        let class_name = self.param.schema.class_name().clone();
        // The collision check is scoped exactly to the schema the new table
        // belongs to: another application may own a same-named table, and a
        // same-named table in the default schema is shadowed, not an error.
        if self
            .meta_mgr
            .get_table_exact(self.param.schema.schema(), &class_name)
            .await?
            .is_some()
        {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                format!("table {} already exists", class_name)
            ));
        }
        Ok(())
    }

    async fn run(&mut self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        trace!(table = %self.param.schema.class_name(), "create_table command run start");
        // Cluster mode: catalog DDL travels the MetaGroup's replicated log so
        // every node applies it; the local apply lands before this returns.
        if let Some(meta_group) = self.x_contract.meta_group() {
            return meta_group
                .apply(::catalog::contract::meta_op::MetaOp::CreateTable {
                    schema: self.param.schema.clone(),
                    binding: self.param.partition_binding.clone(),
                })
                .await;
        }
        self.x_contract
            .create_table(self.param.tx_mgr.clone(), &self.param.schema)
            .await?;
        trace!(table = %self.param.schema.class_name(), "create_table storage create finished");
        if let Some(binding) = &self.param.partition_binding {
            self.meta_mgr.bind_table_partition(binding).await?;
        }
        trace!(table = %self.param.schema.class_name(), "create_table command run done");
        Ok(())
    }
}
