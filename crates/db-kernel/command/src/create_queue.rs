use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{queue_dlq_name, queue_dlq_schema};
use ::xengine::api::XContract;
use ::xengine::x_param::PCreateQueue;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::sync::async_::AMutex;
use std::sync::Arc;
use tracing::trace;

/// Executor of a create-queue command.
pub struct CreateQueue {
    inner: AMutex<_InnerCreateQueue>,
}

struct _InnerCreateQueue {
    param: PCreateQueue,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl CreateQueue {
    /// Creates an executor for the create-queue command `param`, creating
    /// queue storage through `x_contract` and reading and writing the catalog
    /// through `meta_mgr`.
    pub fn new(
        param: PCreateQueue,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            inner: AMutex::new(_InnerCreateQueue::new(param, x_contract, meta_mgr)),
        }
    }
}

#[async_trait]
impl CmdExec for CreateQueue {
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

impl _InnerCreateQueue {
    fn new(
        param: PCreateQueue,
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
        let queue_name = self.param.schema.table_name().clone();
        // Queues share the (schema, name) namespace with tables, so the
        // collision check is scoped exactly to the schema the new queue
        // belongs to, as in create-table.
        for name in [queue_name.clone(), queue_dlq_name(&queue_name)] {
            if self
                .meta_mgr
                .get_table_exact(self.param.schema.schema(), &name)
                .await?
                .is_some()
            {
                return Err(mudu_error!(
                    ER::EntityAlreadyExists,
                    format!("queue {} already exists", name)
                ));
            }
        }
        Ok(())
    }

    async fn run(&mut self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        trace!(queue = %self.param.schema.table_name(), "create_queue command run start");
        self.x_contract
            .create_queue(self.param.tx_mgr.clone(), &self.param.schema)
            .await?;
        // Every queue derives a dead-letter queue with the same payload
        // layout; the DLQ goes through the storage chain directly, so it
        // never derives a DLQ of its own.
        let dlq_schema = queue_dlq_schema(&self.param.schema);
        self.x_contract
            .create_queue(self.param.tx_mgr.clone(), &dlq_schema)
            .await?;
        // The binding lands only after the queue row exists in the catalog
        // (the catalog rejects a binding for a missing table).
        if let Some(binding) = &self.param.partition_binding {
            self.meta_mgr.bind_table_partition(binding).await?;
        }
        // Register the runtime counters of every shard (and of the DLQ's
        // single shard) so the observability endpoint sees the queue before
        // its first enqueue; remote shards prepare on their owners.
        for queue_id in [self.param.schema.id(), dlq_schema.id()] {
            for shard in self.x_contract.queue_shards(queue_id).await? {
                self.x_contract.queue_prepare_shard(&shard).await?;
            }
        }
        trace!(queue = %self.param.schema.table_name(), "create_queue command run done");
        Ok(())
    }
}
