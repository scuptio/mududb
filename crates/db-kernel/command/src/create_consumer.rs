//! Executor of a `CREATE CONSUMER` statement: registers a named consumer
//! cursor on a queue, switching the queue to fan-out mode.
//!
//! The registration inserts one `_queue_consumer` row (frontier 0) in the
//! statement's transaction, so the cursor takes effect at commit and reverts
//! on rollback. In the same transaction the queue's still-`claimed` rows are
//! reset to `ready` with a zeroed `receive_count`: fan-out receives never
//! claim, so a half-claimed leftover from the competing mode would otherwise
//! stay invisible to every cursor (a `claimed` row is only ever re-delivered
//! by a competing RECEIVE or by the restart rebuild).

use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;

use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{
    decode_queue_status, queue_receive_count_datum, queue_status_datum, QUEUE_RECEIVE_COUNT_ATTR,
    QUEUE_SEQ_ATTR, QUEUE_STATUS_ATTR, QUEUE_STATUS_CLAIMED, QUEUE_STATUS_READY,
};
use ::xengine::api::{OptRead, OptUpdate, RangeData, VecDatum, VecSelTerm, XContract};
use ::xengine::x_param::PCreateConsumer;

/// Executor of a create-consumer command.
pub struct CreateConsumer {
    param: PCreateConsumer,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl CreateConsumer {
    /// Creates an executor for the create-consumer command `param`,
    /// registering the cursor through `x_contract` and reading the catalog
    /// through `meta_mgr`.
    pub fn new(
        param: PCreateConsumer,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            param,
            x_contract,
            meta_mgr,
        }
    }
}

#[async_trait]
impl CmdExec for CreateConsumer {
    async fn prepare(&self) -> RS<()> {
        // The binder resolved the queue by name; re-validate that the object
        // still exists and still is a queue.
        let desc = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        if !desc.is_queue() {
            return Err(mudu_error!(
                ER::EntityNotFound,
                format!("no such queue {}", self.param.table_id)
            ));
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        // Re-resolve the queue so a drop between prepare and run fails here.
        self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        // Loads (and after a restart, rebuilds) every shard's runtime state
        // on its owning worker.
        for shard in self.x_contract.queue_shards(self.param.table_id).await? {
            self.x_contract.queue_prepare_shard(&shard).await?;
        }
        // Inserts the `_queue_consumer` rows (frontier 0, one per shard) in
        // the statement's transaction; a duplicate cursor fails with
        // EntityAlreadyExists.
        self.x_contract
            .consumer_create(
                self.param.tx_mgr.clone(),
                self.param.table_id,
                &self.param.consumer,
            )
            .await?;
        self.reset_claimed_rows().await?;
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(0)
    }
}

impl CreateConsumer {
    /// Returns the queue's still-`claimed` rows to `ready` with a zeroed
    /// `receive_count` in the statement's transaction, on every shard.
    /// Fan-out receives are read-only, so claim state has no meaning once
    /// the queue has named consumers; resetting here re-delivers any message
    /// claimed by a competing RECEIVE that never committed its ACK.
    async fn reset_claimed_rows(&self) -> RS<()> {
        let select = VecSelTerm::new(vec![QUEUE_SEQ_ATTR, QUEUE_STATUS_ATTR]);
        for shard in self.x_contract.queue_shards(self.param.table_id).await? {
            let cursor = self
                .x_contract
                .read_range_on_partition(
                    self.param.tx_mgr.clone(),
                    self.param.table_id,
                    shard.partition_id,
                    &RangeData::new(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded),
                    &select,
                    &OptRead::default(),
                )
                .await?;
            while let Some(row) = cursor.next().await? {
                let fields = row.into_fields();
                let (Some(seq_bin), Some(status_bin)) = (fields[0].clone(), fields[1].clone())
                else {
                    continue;
                };
                if decode_queue_status(&status_bin)? != QUEUE_STATUS_CLAIMED {
                    continue;
                }
                self.x_contract
                    .update_on_partition(
                        self.param.tx_mgr.clone(),
                        self.param.table_id,
                        shard.partition_id,
                        &VecDatum::new(vec![(QUEUE_SEQ_ATTR, seq_bin)]),
                        &VecDatum::new(vec![
                            (QUEUE_STATUS_ATTR, queue_status_datum(QUEUE_STATUS_READY)?),
                            (QUEUE_RECEIVE_COUNT_ATTR, queue_receive_count_datum(0)?),
                        ]),
                        &OptUpdate::default(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
