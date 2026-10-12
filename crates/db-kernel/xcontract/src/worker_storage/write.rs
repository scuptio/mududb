use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;

use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::index::index_key::key_tuple::KeyTuple;
use ::xengine::tx_mgr::TxMgr;

use super::commit::write_version_to_kv_store;
use super::WorkerStorage;

impl WorkerStorage {
    #[cfg(test)]
    pub(crate) async fn put(
        &self,
        oid: OID,
        key: Vec<u8>,
        value: Vec<u8>,
        txm: &dyn TxMgr,
    ) -> RS<()> {
        self.put_on_partition(oid, None, key, value, txm).await
    }

    pub(crate) async fn put_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: Vec<u8>,
        value: Vec<u8>,
        txm: &dyn TxMgr,
    ) -> RS<()> {
        let key_tuple = KeyTuple::from(key.clone());
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));

        // The statement-level lock supersedes the snapshot conflict check
        // for keys the transaction already holds: it was re-read at the
        // latest committed version under the lock.
        if !txm.has_statement_lock(&relation_id, key.as_slice()) {
            self.ensure_no_relation_write_conflict(oid, partition_id, &key_tuple, &txm.snapshot())
                .await?;
        }
        txm.put_relation(relation_id, key, value);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn remove(
        &self,
        oid: OID,
        key: &[u8],
        txm: &dyn TxMgr,
    ) -> RS<Option<Vec<u8>>> {
        self.remove_on_partition(oid, None, key, txm).await
    }

    pub(crate) async fn remove_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        txm: &dyn TxMgr,
    ) -> RS<Option<Vec<u8>>> {
        let key_tuple = KeyTuple::from(key.to_vec());
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        let statement_locked = txm.has_statement_lock(&relation_id, key);
        if !statement_locked {
            self.ensure_no_relation_write_conflict(oid, partition_id, &key_tuple, &txm.snapshot())
                .await?;
        }
        let current = match txm.get_relation(relation_id, key) {
            Some(staged) => staged,
            None => {
                let snapshot = if statement_locked {
                    WorkerSnapshot::latest_committed()
                } else {
                    txm.snapshot()
                };
                self.read_visible_relation_value(oid, partition_id, &key_tuple, &snapshot)
                    .await?
            }
        };
        if current.is_some() {
            txm.delete_relation(relation_id, key.to_vec());
        }
        Ok(current)
    }

    pub(crate) fn worker_put_local(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        write_version_to_kv_store(&self.kv_store, key, Some(value), xid)
    }

    pub(crate) fn worker_delete_local(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        write_version_to_kv_store(&self.kv_store, key, None, xid)
    }

    async fn ensure_no_relation_write_conflict(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<()> {
        scoped_task_trace!();
        self.ensure_relation_index(oid, partition_id).await?;
        let relation = self.get_relation_async(oid, partition_id).await?;
        if relation.has_write_conflict(key, snapshot).await? {
            return Err(mudu_error!(
                ErrorCode::Transaction,
                format!(
                    "write-write conflict on table {} key {:?} for transaction {}",
                    oid,
                    key.as_slice(),
                    snapshot.xid()
                )
            ));
        }
        Ok(())
    }
}
