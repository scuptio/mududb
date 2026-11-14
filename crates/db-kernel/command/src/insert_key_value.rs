use crate::fs_hook;
use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{
    queue_lsn_datum, queue_receive_count_datum, queue_seq_datum, queue_seq_encode,
    queue_status_datum, QUEUE_LSN_ATTR, QUEUE_RECEIVE_COUNT_ATTR, QUEUE_SEQ_ATTR,
    QUEUE_STATUS_ATTR, QUEUE_STATUS_READY,
};
use ::xengine::api::{OptInsert, QueueShard, VecDatum, XContract};
use ::xengine::x_param::PInsertKeyValue;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// Executor of an insert command.
pub struct InsertKeyValue {
    param: PInsertKeyValue,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
    affected_rows: AtomicU64,
}

impl InsertKeyValue {
    /// Creates an executor for the insert command `param`, inserting rows
    /// through `x_contract` and reading the catalog through `meta_mgr`.
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
        let desc = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        if desc.is_queue() {
            // Queue rows carry no user-provided key: `__seq` and the value
            // header columns are synthesized at run time.
            return Ok(());
        }
        for (key, _value) in &self.param.rows {
            if key.data().is_empty() {
                // An empty insert key means the table has no primary key
                // columns (the binder binds every key column into the row):
                // the KV engine cannot address keyless rows, and equal empty
                // keys would make every row overwrite the previous one.
                // Reject with InvalidTuple (a user input/shape error), not
                // EntityNotFound (nothing was looked up and missed here).
                if desc.key_indices().is_empty() {
                    return Err(mudu_error!(
                        ER::InvalidTuple,
                        format!(
                            "insert into table {} without primary key is not supported",
                            desc.name()
                        )
                    ));
                }
                return Err(mudu_error!(ER::InvalidTuple, "insert key is empty"));
            }
        }
        Ok(())
    }

    async fn insert_inner(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let desc = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        if desc.is_queue() {
            return self.enqueue_queue_rows().await;
        }
        let mut affected_rows = 0;
        for (key, value) in &self.param.rows {
            if fs_hook::has_fs_bound_columns(desc.as_ref()) {
                let mut value = value.clone();
                let staged = fs_hook::bind_fs_columns_on_insert(
                    &self.meta_mgr,
                    &self.x_contract,
                    self.param.table_id,
                    desc.as_ref(),
                    key,
                    &mut value,
                )
                .await?;
                self.x_contract
                    .insert(
                        self.param.tx_mgr.clone(),
                        self.param.table_id,
                        key,
                        &value,
                        &OptInsert::default(),
                    )
                    .await?;
                fs_hook::stage_fs_ops(&self.param.tx_mgr, staged);
            } else {
                self.x_contract
                    .insert(
                        self.param.tx_mgr.clone(),
                        self.param.table_id,
                        key,
                        value,
                        &OptInsert::default(),
                    )
                    .await?;
            }
            affected_rows += 1;
        }
        self.affected_rows.store(affected_rows, Ordering::Relaxed);
        Ok(())
    }

    /// Enqueue path for queue tables: the user rows carry payload columns
    /// only; the kernel routes each row to its shard (the queue's partition
    /// column, or the single shard of an unpartitioned queue), reserves
    /// per-shard local sequences, and synthesizes the system columns
    /// (`__seq` = shard bits | local sequence, `__lsn` from the enclosing
    /// transaction's xid — which becomes its commit version —
    /// `status = ready`, `receive_count = 0`), inserting the assembled rows
    /// on their shard partitions. The per-shard depth cap surfaces as
    /// [`ER::QueueFull`], which fails the enqueueing transaction.
    async fn enqueue_queue_rows(&self) -> RS<()> {
        let rows = &self.param.rows;
        if rows.is_empty() {
            return Ok(());
        }
        // Group by shard, preserving per-shard statement order.
        let mut per_shard: Vec<(QueueShard, Vec<&VecDatum>)> = Vec::new();
        for (_key, value) in rows {
            let shard = self
                .x_contract
                .queue_route_payload(self.param.table_id, value)
                .await?;
            match per_shard
                .iter_mut()
                .find(|(candidate, _)| *candidate == shard)
            {
                Some((_, rows)) => rows.push(value),
                None => per_shard.push((shard, vec![value])),
            }
        }
        let lsn = queue_lsn_datum(self.param.tx_mgr.xid())?;
        let mut affected_rows = 0;
        for (shard, shard_rows) in per_shard {
            self.x_contract.queue_prepare_shard(&shard).await?;
            let bytes = shard_rows
                .iter()
                .flat_map(|value| value.data().iter())
                .map(|(_, bin)| bin.len() as u64)
                .sum();
            let first_local = self
                .x_contract
                .queue_reserve(
                    self.param.tx_mgr.clone(),
                    &shard,
                    shard_rows.len() as u64,
                    bytes,
                )
                .await?;
            for (index, value) in shard_rows.iter().enumerate() {
                let seq = queue_seq_encode(shard.shard_index, first_local + index as u64)?;
                let key = VecDatum::new(vec![(QUEUE_SEQ_ATTR, queue_seq_datum(seq)?)]);
                let mut full_value = vec![
                    (QUEUE_LSN_ATTR, lsn.clone()),
                    (QUEUE_STATUS_ATTR, queue_status_datum(QUEUE_STATUS_READY)?),
                    (QUEUE_RECEIVE_COUNT_ATTR, queue_receive_count_datum(0)?),
                ];
                full_value.extend(value.data().iter().cloned());
                self.x_contract
                    .insert_on_partition(
                        self.param.tx_mgr.clone(),
                        self.param.table_id,
                        shard.partition_id,
                        &key,
                        &VecDatum::new(full_value),
                    )
                    .await?;
                affected_rows += 1;
            }
        }
        self.affected_rows.store(affected_rows, Ordering::Relaxed);
        Ok(())
    }
}
