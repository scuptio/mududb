use super::utils::is_cross_partition_tx;
use super::*;

#[async_trait]
impl XContract for WorkerXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, schema: &SchemaTable) -> RS<()> {
        self.storage.create_table_async(schema).await
    }

    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, oid: OID) -> RS<()> {
        self.storage.drop_table_async(oid).await
    }

    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &AlterTable,
    ) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "alter table is not implemented"
        ))
    }

    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        self._begin_tx()
    }

    async fn commit_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        if is_cross_partition_tx(tx_mgr.as_ref()) {
            return self.worker_commit_cross_partition_tx_async(tx_mgr).await;
        }
        self.worker_commit_tx_async(tx_mgr).await
    }

    async fn abort_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        self.worker_rollback_tx(tx_mgr)
    }

    async fn update(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._update(desc, tx_mgr, table_id, pred_key, pred_non_key, values)
            .await
    }

    async fn read_key(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<DatBin>>>> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._read_key(desc, tx_mgr, table_id, pred_key, select)
            .await
    }

    async fn read_range(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._read_range(desc, tx_mgr, table_id, pred_key, pred_non_key, select)
            .await
    }

    async fn delete(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        opt_delete: &OptDelete,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._delete(desc, tx_mgr, table_id, pred_key, pred_non_key, opt_delete)
            .await
    }

    async fn insert(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        opt_insert: &OptInsert,
    ) -> RS<()> {
        scoped_task_trace!();
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._insert(desc, tx_mgr, table_id, keys, values, opt_insert)
            .await
    }
}
