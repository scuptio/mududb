use mudu_sys::sync::SMutex;
use std::collections::{BTreeMap, Bound};
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::sync::{Arc, OnceLock, Weak};

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_utils::{scoped_task_trace, task_trace};
use scc::HashMap as SccHashMap;

use crate::contract::data_row::DataRow;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule_binding::TablePartitionBinding;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::timestamp::Timestamp;
use crate::contract::version_tuple::VersionTuple;
use crate::index::index_key::key_tuple::KeyTuple;
use crate::server::partition_router::DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID;
use crate::server::worker_snapshot::{KvItem, WorkerSnapshot};
use crate::server::worker_tx_manager::WorkerTxManager;
use crate::storage::relation::relation::Relation;
use crate::wal::xl_batch::XLBatch;
use crate::wal::xl_data_op::{XLDelete, XLInsert, XLWrite};
use crate::wal::xl_entry::TxOp;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use tracing::{info, trace};

type WorkerStorageRegistry = std::collections::HashMap<String, Vec<Weak<WorkerStorage>>>;

fn storage_registry() -> &'static SMutex<WorkerStorageRegistry> {
    static REGISTRY: OnceLock<SMutex<WorkerStorageRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(std::collections::HashMap::new()))
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedWorkerCommit {
    xid: u64,
    relation_rows: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    kv_rows: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    batch: XLBatch,
}

pub struct WorkerStorage {
    mgr: Arc<dyn MetaMgr>,
    default_partition_id: OID,
    relation_path: String,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    relation_store: SccHashMap<PhysicalRelationId, Arc<Relation>>,
    kv_store: SccHashMap<Vec<u8>, DataRow>,
    applied_cross_tx: SccHashMap<OID, ()>,
}

impl WorkerStorage {
    fn relation_id(&self, table_id: OID, partition_id: OID) -> PhysicalRelationId {
        PhysicalRelationId {
            table_id,
            partition_id,
        }
    }

    #[allow(dead_code)]
    pub fn new(mgr: Arc<dyn MetaMgr>, partition_id: OID, relation_path: String) -> Self {
        Self::new_with_async_runtime(mgr, partition_id, relation_path, None)
    }

    pub fn new_with_async_runtime(
        mgr: Arc<dyn MetaMgr>,
        partition_id: OID,
        relation_path: String,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> Self {
        Self {
            mgr,
            default_partition_id: partition_id,
            relation_path,
            async_runtime,
            relation_store: SccHashMap::new(),
            kv_store: SccHashMap::new(),
            applied_cross_tx: SccHashMap::new(),
        }
    }

    fn physical_partition_id(&self, partition_id: Option<OID>) -> OID {
        partition_id.unwrap_or(self.default_partition_id)
    }

    pub fn register_global(self: &Arc<Self>) {
        let mut guard = storage_registry().lock().unwrap();
        guard
            .entry(self.relation_path.clone())
            .or_default()
            .push(Arc::downgrade(self));
    }

    pub async fn bootstrap_existing_tables_async(&self) -> RS<()> {
        for schema in self.mgr.list_schemas().await? {
            self.bootstrap_table_local_async(&schema).await?;
        }
        Ok(())
    }

    pub async fn create_table_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage create_table_async start");
        self.mgr.create_table(schema).await?;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage metadata create finished");
        let r = self.broadcast_create_table_async(schema).await;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async finished");
        r
    }

    pub async fn drop_table_async(&self, oid: OID) -> RS<()> {
        self.mgr.drop_table(oid).await?;
        self.broadcast_drop_table_async(oid).await
    }

    #[allow(dead_code)]
    pub async fn contains_key(&self, oid: OID, key: &KeyTuple, txm: &dyn TxMgr) -> RS<bool> {
        self.contains_key_on_partition(oid, None, key, txm).await
    }

    pub async fn contains_key_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        txm: &dyn TxMgr,
    ) -> RS<bool> {
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        if let Some(staged) = txm.get_relation(relation_id, key.as_slice()) {
            return Ok(staged.is_some());
        }
        self.read_visible_relation_exists(oid, partition_id, key, &txm.snapshot())
            .await
    }

    #[allow(dead_code)]
    pub async fn get(&self, oid: OID, key: &[u8], txm: &dyn TxMgr) -> RS<Option<Vec<u8>>> {
        self.get_on_partition(oid, None, key, txm).await
    }

    pub async fn get_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        txm: &dyn TxMgr,
    ) -> RS<Option<Vec<u8>>> {
        let trace = task_trace!();
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        trace.watch("storage.get.table_id", &oid.to_string());
        trace.watch("storage.get.relation_id", &format!("{relation_id:?}"));
        trace.watch(
            "storage.get.partition_id",
            &partition_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string()),
        );
        trace.watch("storage.get.stage", "tx_lookup_call");
        if let Some(staged) = txm.get_relation(relation_id, key) {
            trace.watch("storage.get.stage", "tx_lookup_hit");
            return Ok(staged);
        }
        trace.watch("storage.get.stage", "tx_lookup_miss");
        let key = KeyTuple::from(key.to_vec());
        trace.watch("storage.get.stage", "visible_read");
        self.read_visible_relation_value(oid, partition_id, &key, &txm.snapshot())
            .await
    }

    #[allow(dead_code)]
    pub async fn put(&self, oid: OID, key: Vec<u8>, value: Vec<u8>, txm: &dyn TxMgr) -> RS<()> {
        self.put_on_partition(oid, None, key, value, txm).await
    }

    pub async fn put_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: Vec<u8>,
        value: Vec<u8>,
        txm: &dyn TxMgr,
    ) -> RS<()> {
        let key_tuple = KeyTuple::from(key.clone());
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));

        self.ensure_no_relation_write_conflict(oid, partition_id, &key_tuple, &txm.snapshot())
            .await?;
        txm.put_relation(relation_id, key, value);
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn remove(&self, oid: OID, key: &[u8], txm: &dyn TxMgr) -> RS<Option<Vec<u8>>> {
        self.remove_on_partition(oid, None, key, txm).await
    }

    pub async fn remove_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        txm: &dyn TxMgr,
    ) -> RS<Option<Vec<u8>>> {
        let key_tuple = KeyTuple::from(key.to_vec());
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        self.ensure_no_relation_write_conflict(oid, partition_id, &key_tuple, &txm.snapshot())
            .await?;
        let current = match txm.get_relation(relation_id, key) {
            Some(staged) => staged,
            None => {
                self.read_visible_relation_value(oid, partition_id, &key_tuple, &txm.snapshot())
                    .await?
            }
        };
        if current.is_some() {
            txm.delete_relation(relation_id, key.to_vec());
        }
        Ok(current)
    }

    pub async fn range(
        &self,
        oid: OID,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        txm: &dyn TxMgr,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.range_on_partition(oid, None, bounds, txm).await
    }

    pub async fn range_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        txm: &dyn TxMgr,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let base_items = self
            .range_visible_relation(oid, partition_id, bounds, &txm.snapshot())
            .await?;
        let (start_key, end_key) = bounds_to_scan(&bounds);
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        let staged_items = txm.staged_relation_items_in_range(relation_id, &start_key, &end_key);

        let mut merged = BTreeMap::new();
        for (key, value) in base_items {
            merged.insert(key, Some(value));
        }
        for (key, value) in staged_items {
            merged.insert(key, value);
        }

        Ok(merged
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect())
    }

    pub async fn kv_get(
        &self,
        key: &[u8],
        snapshot: Option<&WorkerSnapshot>,
    ) -> RS<Option<Vec<u8>>> {
        let row = self.kv_store.get_sync(key).map(|entry| entry.get().clone());
        let version = match snapshot {
            Some(snapshot) => match row {
                Some(row) => {
                    let snapshot = snapshot.to_snapshot();
                    row.read(&snapshot).await?
                }
                None => None,
            },
            None => match row {
                Some(row) => row.read_latest().await?,
                None => None,
            },
        };
        Ok(version
            .filter(|version| !version.is_deleted())
            .map(|version| version.tuple().clone()))
    }

    pub async fn kv_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        snapshot: Option<&WorkerSnapshot>,
    ) -> RS<Vec<KvItem>> {
        let mut rows = Vec::new();
        self.kv_store.iter_sync(|key, row| {
            let in_range = if end_key.is_empty() {
                key.as_slice() >= start_key
            } else {
                key.as_slice() >= start_key && key.as_slice() < end_key
            };
            if in_range {
                rows.push((key.clone(), row.clone()));
            }
            true
        });

        let mut items = Vec::new();
        for (key, row) in rows {
            let visible = match snapshot {
                Some(snapshot) => {
                    let snapshot = snapshot.to_snapshot();
                    row.read(&snapshot).await?
                }
                None => row.read_latest().await?,
            };
            if let Some(visible) = visible.filter(|version| !version.is_deleted()) {
                items.push(KvItem {
                    key,
                    value: visible.tuple().clone(),
                });
            }
        }
        items.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(items)
    }

    #[allow(dead_code)]
    pub(crate) async fn commit_tx(&self, txm: &mut WorkerTxManager) -> RS<()> {
        let prepared = self.prepare_commit_async(txm).await?;
        self.apply_relation_rows_async(&prepared).await?;
        self.apply_kv_rows_async(&prepared).await?;
        Ok(())
    }

    pub(crate) async fn prepare_commit_async(&self, txm: &dyn TxMgr) -> RS<PreparedWorkerCommit> {
        self.prepare_commit_parts_async(
            &txm.snapshot(),
            txm.xid(),
            txm.staged_relation_ops(),
            txm.staged_put_items().into_iter().collect(),
            txm.xl_batch(),
        )
        .await
    }

    pub(crate) async fn prepare_worker_kv_commit(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<PreparedWorkerCommit> {
        self.prepare_commit_parts_async(snapshot, xid, BTreeMap::new(), items, batch)
            .await
    }

    pub(crate) fn prepare_worker_kv_autocommit(
        &self,
        xid: u64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        batch: XLBatch,
    ) -> PreparedWorkerCommit {
        PreparedWorkerCommit {
            xid,
            relation_rows: BTreeMap::new(),
            kv_rows: BTreeMap::from([(key, value)]),
            batch,
        }
    }

    pub(crate) async fn apply_prepared_commit_async(
        &self,
        prepared: PreparedWorkerCommit,
    ) -> RS<()> {
        scoped_task_trace!();
        self.apply_relation_rows_async(&prepared).await?;
        self.apply_kv_rows_async(&prepared).await?;
        Ok(())
    }

    pub(crate) async fn apply_cross_partition_tx_async(
        &self,
        tx_id: OID,
        partition_write_set: &[XLWrite],
    ) -> RS<()> {
        if self.applied_cross_tx.contains_async(&tx_id).await {
            return Ok(());
        }
        for write in partition_write_set {
            match write {
                XLWrite::Insert(insert) => {
                    self.apply_relation_replay_insert_async(insert.clone(), tx_id as u64)
                        .await?;
                }
                XLWrite::Delete(delete) => {
                    self.apply_relation_replay_delete_async(delete.clone(), tx_id as u64)
                        .await?;
                }
                XLWrite::Update(_) => {
                    return Err(m_error!(
                        EC::NotImplemented,
                        "cross-partition update replay is not implemented"
                    ));
                }
            }
        }
        let _ = self.applied_cross_tx.insert_async(tx_id, ()).await;
        Ok(())
    }

    pub(crate) async fn replay_batch(&self, batch: XLBatch) -> RS<()> {
        for entry in batch.entries {
            for op in entry.ops {
                match op {
                    TxOp::Write(XLWrite::Insert(insert))
                        if insert.table_id == 0 && insert.partition_id == 0 =>
                    {
                        self.worker_put_local(insert.key, insert.value, entry.xid)?;
                    }
                    TxOp::Write(XLWrite::Delete(delete))
                        if delete.table_id == 0 && delete.partition_id == 0 =>
                    {
                        self.worker_delete_local(delete.key, entry.xid)?;
                    }
                    TxOp::Write(XLWrite::Insert(insert)) => {
                        self.apply_relation_replay_insert_async(insert, entry.xid)
                            .await?;
                    }
                    TxOp::Write(XLWrite::Delete(delete)) => {
                        self.apply_relation_replay_delete_async(delete, entry.xid)
                            .await?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub(crate) fn worker_put_local(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        write_version_to_kv_store(&self.kv_store, key, Some(value), xid)
    }

    pub(crate) fn worker_delete_local(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        write_version_to_kv_store(&self.kv_store, key, None, xid)
    }

    async fn prepare_commit_parts_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        relation_rows: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
        kv_rows: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<PreparedWorkerCommit> {
        self.ensure_no_relation_conflicts_async(snapshot, xid, &relation_rows)
            .await?;
        self.ensure_no_kv_conflicts(snapshot, xid, &kv_rows)?;

        Ok(PreparedWorkerCommit {
            xid,
            relation_rows,
            kv_rows,
            batch,
        })
    }

    async fn ensure_no_relation_conflicts_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        relation_rows: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    ) -> RS<()> {
        for (relation_id, rows) in relation_rows {
            let relation = self.get_relation_by_id_async(relation_id).await?;
            for key in rows.keys() {
                let key_tuple = KeyTuple::from(key.clone());
                if relation.has_write_conflict(&key_tuple, snapshot).await? {
                    return Err(m_error!(
                        EC::TxErr,
                        format!(
                            "write-write conflict on table {} partition {} key {:?} for transaction {}",
                            relation_id.table_id, relation_id.partition_id, key, xid
                        )
                    ));
                }
            }
        }
        Ok(())
    }

    fn ensure_no_kv_conflicts(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        kv_rows: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    ) -> RS<()> {
        for key in kv_rows.keys() {
            let conflict = self
                .kv_store
                .get_sync(key)
                .and_then(|entry| latest_version(entry.get()))
                .map(|latest| !snapshot.is_visible(latest.timestamp().c_min()))
                .unwrap_or(false);
            if conflict {
                return Err(m_error!(
                    EC::TxErr,
                    format!(
                        "write-write conflict on key {:?} for transaction {}",
                        String::from_utf8_lossy(key),
                        xid
                    )
                ));
            }
        }
        Ok(())
    }

    async fn apply_relation_rows_async(&self, prepared: &PreparedWorkerCommit) -> RS<()> {
        scoped_task_trace!();
        for (relation_id, rows) in &prepared.relation_rows {
            let relation = self.get_relation_by_id_async(relation_id).await?;
            for (key, value) in rows {
                relation
                    .write_row(key.clone(), value.clone(), prepared.xid)
                    .await?;
            }
        }
        Ok(())
    }

    async fn apply_kv_rows_async(&self, prepared: &PreparedWorkerCommit) -> RS<()> {
        scoped_task_trace!();
        for (key, value) in &prepared.kv_rows {
            write_version_to_kv_store_async(
                &self.kv_store,
                key.clone(),
                value.clone(),
                prepared.xid,
            )
            .await?;
        }
        Ok(())
    }

    async fn apply_relation_replay_insert_async(&self, insert: XLInsert, xid: u64) -> RS<()> {
        let relation = self
            .get_relation_by_id_async(&self.relation_id(insert.table_id, insert.partition_id))
            .await?;
        relation.write_value(insert.key, insert.value, xid).await
    }

    async fn apply_relation_replay_delete_async(&self, delete: XLDelete, xid: u64) -> RS<()> {
        let relation = self
            .get_relation_by_id_async(&self.relation_id(delete.table_id, delete.partition_id))
            .await?;
        relation.write_delete(delete.key, xid).await
    }

    #[allow(dead_code)]
    async fn read_visible_relation_exists(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<bool> {
        let relation = self.get_relation_async(oid, partition_id).await?;
        relation.has_visible_version(key, snapshot).await
    }

    async fn read_visible_relation_value(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        self.ensure_relation_index(oid, partition_id).await?;
        let relation = self.get_relation_async(oid, partition_id).await?;
        relation.visible_value(key, snapshot).await
    }

    async fn range_visible_relation(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.ensure_relation_index(oid, partition_id).await?;
        let relation = self.get_relation_async(oid, partition_id).await?;
        relation.visible_range(bounds, snapshot).await
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
            return Err(m_error!(
                EC::TxErr,
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

    async fn create_relation_index_for_partition_async(
        &self,
        oid: OID,
        partition_id: OID,
        table_desc: &TableDesc,
    ) -> RS<()> {
        scoped_task_trace!();
        let relation_id = self.relation_id(oid, partition_id);

        let relation = match &self.async_runtime {
            Some(async_runtime) => Arc::new(
                Relation::new_with_provider(
                    async_runtime.clone(),
                    oid,
                    partition_id,
                    self.relation_path.clone(),
                    table_desc,
                )
                .await?,
            ),
            None => Arc::new(
                Relation::new(oid, partition_id, self.relation_path.clone(), table_desc).await?,
            ),
        };

        let _ = self
            .relation_store
            .insert_async(relation_id, relation)
            .await;
        Ok(())
    }

    async fn ensure_relation_index(&self, oid: OID, partition_id: Option<OID>) -> RS<()> {
        scoped_task_trace!();
        let partition_id = self.physical_partition_id(partition_id);
        let relation_id = self.relation_id(oid, partition_id);
        if self.relation_store.contains_async(&relation_id).await {
            return Ok(());
        }

        let table_desc = self.mgr.get_table_by_id(oid).await?;
        self.create_relation_index_for_partition_async(oid, partition_id, table_desc.as_ref())
            .await?;
        Ok(())
    }

    async fn apply_create_table_local_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage apply_create_table_local_async start");
        let table_desc =
            crate::contract::table_info::TableInfo::new(schema.clone())?.table_desc()?;
        self.create_relation_index_for_partition_async(
            schema.id(),
            DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
            table_desc.as_ref(),
        )
        .await?;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage apply_create_table_local_async done");
        Ok(())
    }

    async fn bootstrap_table_local_async(&self, schema: &SchemaTable) -> RS<()> {
        let table_desc =
            crate::contract::table_info::TableInfo::new(schema.clone())?.table_desc()?;
        let binding = self.mgr.get_table_partition_binding(schema.id()).await?;
        match binding {
            Some(binding) => {
                self.create_partitioned_relations_async(schema.id(), &binding, table_desc.as_ref())
                    .await
            }
            None => {
                self.create_relation_index_for_partition_async(
                    schema.id(),
                    DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                    table_desc.as_ref(),
                )
                .await
            }
        }
    }

    async fn create_partitioned_relations_async(
        &self,
        oid: OID,
        binding: &TablePartitionBinding,
        table_desc: &TableDesc,
    ) -> RS<()> {
        let rule = self.mgr.get_partition_rule_by_id(binding.rule_id).await?;
        for partition in &rule.partitions {
            self.create_relation_index_for_partition_async(oid, partition.partition_id, table_desc)
                .await?;
        }
        Ok(())
    }

    fn apply_drop_table_local(&self, oid: OID) {
        scoped_task_trace!();
        let relation_id = self.relation_id(oid, self.default_partition_id);
        let _removed = self.relation_store.remove_sync(&relation_id);
    }

    async fn apply_drop_table_local_async(&self, oid: OID) {
        let trace = task_trace!();
        let task_id = mudu_sys::task::async_::try_this_task_id();
        let relation_id = self.relation_id(oid, self.default_partition_id);
        trace.watch("relation_store.op", "remove_async");
        trace.watch("relation_store.phase", "before_remove");
        trace.watch("relation_store.relation_id", &format!("{relation_id:?}"));
        trace.watch("relation_store.task_id", &format!("{task_id:?}"));
        let removed = self.relation_store.remove_async(&relation_id).await;
        trace.watch("relation_store.phase", "after_remove");
        info!(
            task_id = ?task_id,
            relation_id = ?relation_id,
            existed = removed.is_some(),
            "relation_store remove_async"
        );
    }

    async fn broadcast_create_table_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async enter");
        let peers = self.peer_instances();
        if peers.is_empty() {
            return self.apply_create_table_local_async(schema).await;
        }
        for storage in peers {
            storage.apply_create_table_local_async(schema).await?;
        }
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async done");
        Ok(())
    }

    #[allow(dead_code)]
    fn broadcast_drop_table(&self, oid: OID) -> RS<()> {
        let peers = self.peer_instances();
        if peers.is_empty() {
            self.apply_drop_table_local(oid);
            return Ok(());
        }
        for storage in peers {
            storage.apply_drop_table_local(oid);
        }
        Ok(())
    }

    async fn get_relation_async(&self, oid: OID, partition_id: Option<OID>) -> RS<Arc<Relation>> {
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        self.get_relation_by_id_async(&relation_id).await
    }

    async fn get_relation_by_id_async(
        &self,
        relation_id: &PhysicalRelationId,
    ) -> RS<Arc<Relation>> {
        self.relation_store
            .get_async(relation_id)
            .await
            .map(|relation| relation.get().clone())
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!(
                        "no such table {} partition {}",
                        relation_id.table_id, relation_id.partition_id
                    )
                )
            })
    }

    async fn broadcast_drop_table_async(&self, oid: OID) -> RS<()> {
        let peers = self.peer_instances();
        if peers.is_empty() {
            self.apply_drop_table_local_async(oid).await;
            return Ok(());
        }
        for storage in peers {
            storage.apply_drop_table_local_async(oid).await;
        }
        Ok(())
    }

    fn peer_instances(&self) -> Vec<Arc<WorkerStorage>> {
        let mut guard = storage_registry().lock().unwrap();
        let peers = guard.entry(self.relation_path.clone()).or_default();
        let mut live = Vec::with_capacity(peers.len());
        peers.retain(|weak| match weak.upgrade() {
            Some(storage) => {
                live.push(storage);
                true
            }
            None => false,
        });
        live
    }
}

impl PreparedWorkerCommit {
    pub(crate) fn batch(&self) -> &XLBatch {
        &self.batch
    }
}

fn new_value_version(xid: u64, value: Vec<u8>) -> VersionTuple {
    VersionTuple::new(Timestamp::new(xid, u64::MAX), value)
}

fn write_version_to_kv_store(
    kv_store: &SccHashMap<Vec<u8>, DataRow>,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    xid: u64,
) -> RS<()> {
    let row = kv_store
        .get_sync(&key)
        .map(|entry| entry.get().clone())
        .unwrap_or_else(|| DataRow::new(0));
    let version = match value {
        Some(value) => new_value_version(xid, value),
        None => VersionTuple::new_delete(Timestamp::new(xid, u64::MAX)),
    };
    row.write_sync(version, None)?;
    let _ = kv_store.insert_sync(key, row);
    Ok(())
}

async fn write_version_to_kv_store_async(
    kv_store: &SccHashMap<Vec<u8>, DataRow>,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    xid: u64,
) -> RS<()> {
    scoped_task_trace!();
    let row = kv_store
        .get_sync(&key)
        .map(|entry| entry.get().clone())
        .unwrap_or_else(|| DataRow::new(0));
    let version = match value {
        Some(value) => new_value_version(xid, value),
        None => VersionTuple::new_delete(Timestamp::new(xid, u64::MAX)),
    };
    row.write(version, None).await?;
    let _ = kv_store.insert_sync(key, row);
    Ok(())
}

fn latest_version(row: &DataRow) -> Option<VersionTuple> {
    row.read_latest_sync().ok().flatten()
}

fn bounds_to_scan(bounds: &(Bound<&[u8]>, Bound<&[u8]>)) -> (Vec<u8>, Vec<u8>) {
    let start = match bounds.0 {
        Included(key) | Excluded(key) => key.to_vec(),
        Unbounded => Vec::new(),
    };
    let end = match bounds.1 {
        Included(key) | Excluded(key) => key.to_vec(),
        Unbounded => Vec::new(),
    };
    (start, end)
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use crate::contract::schema_column::SchemaColumn;
    use crate::server::test_meta_mgr::TestMetaMgr;
    use mudu::common::id::OID;
    use mudu_sys::common::provider_type::ProviderType;
    use mudu_sys::provider::create_io_provider;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dt_info::DTInfo;

    use super::*;

    fn block_on<F>(fut: F) -> F::Output
    where
        F: Future,
    {
        mudu_sys::task::async_::build_current_thread_runtime()
            .unwrap()
            .block_on(fut)
    }

    fn test_schema() -> SchemaTable {
        SchemaTable::new(
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_text(DatTypeID::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_text(DatTypeID::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        )
    }

    async fn test_storage() -> RS<(WorkerStorage, OID)> {
        let mgr = Arc::new(TestMetaMgr::new());
        let storage = WorkerStorage::new(
            mgr,
            0,
            mudu_sys::env_var::temp_dir()
                .join(format!(
                    "worker_storage_test_{}",
                    mudu_utils::oid::gen_oid()
                ))
                .to_string_lossy()
                .to_string(),
        );
        let schema = test_schema();
        let oid = schema.id();
        storage.create_table_async(&schema).await?;
        Ok((storage, oid))
    }

    async fn test_shared_storage() -> RS<(
        Arc<TestMetaMgr>,
        Arc<WorkerStorage>,
        Arc<WorkerStorage>,
        OID,
    )> {
        let mgr = Arc::new(TestMetaMgr::new());
        let root = mudu_sys::env_var::temp_dir()
            .join(format!(
                "worker_storage_shared_test_{}",
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .to_string();
        let storage1 = Arc::new(WorkerStorage::new(mgr.clone(), 1, root.clone()));
        storage1.register_global();
        storage1.bootstrap_existing_tables_async().await?;
        let storage2 = Arc::new(WorkerStorage::new(mgr.clone(), 2, root));
        storage2.register_global();
        storage2.bootstrap_existing_tables_async().await?;

        let schema = test_schema();
        let oid = schema.id();
        storage1.create_table_async(&schema).await?;
        Ok((mgr, storage1, storage2, oid))
    }

    fn begin_tx(xid: u64, running: Vec<u64>) -> WorkerTxManager {
        WorkerTxManager::new(WorkerSnapshot::new(xid, running))
    }

    fn i32_bytes(v: i32) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    #[test]
    fn worker_storage_broadcasts_create_and_drop_to_peer_workers() {
        block_on(async move {
            let r = _worker_storage_broadcasts_create_and_drop_to_peer_workers().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_broadcasts_create_and_drop_to_peer_workers() -> RS<()> {
        let (mgr, _storage1, storage2, oid) = test_shared_storage().await?;
        let mut tx = begin_tx(1, vec![]);
        storage2
            .put(oid, i32_bytes(7), i32_bytes(70), &mut tx)
            .await?;
        storage2.commit_tx(&mut tx).await?;
        assert!(mgr.get_table_by_id(oid).await.is_ok());

        storage2.drop_table_async(oid).await?;
        assert!(mgr.get_table_by_id(oid).await.is_err());

        let mut tx = begin_tx(2, vec![]);
        let err = storage2
            .put(oid, i32_bytes(8), i32_bytes(80), &mut tx)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("no such table"));
        Ok(())
    }

    #[test]
    fn worker_storage_bootstraps_existing_tables_with_async_runtime() {
        block_on(async move {
            let r = _worker_storage_bootstraps_existing_tables_with_async_runtime().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_bootstraps_existing_tables_with_async_runtime() -> RS<()> {
        let mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let oid = schema.id();
        mgr.create_table(&schema).await?;
        let storage = WorkerStorage::new_with_async_runtime(
            mgr,
            0,
            mudu_sys::env_var::temp_dir()
                .join(format!(
                    "worker_storage_async_bootstrap_test_{}",
                    mudu_utils::oid::gen_oid()
                ))
                .to_string_lossy()
                .to_string(),
            Some(create_io_provider(ProviderType::Tokio)),
        );
        storage.bootstrap_existing_tables_async().await?;

        let mut tx = begin_tx(1, vec![]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(10), &mut tx)
            .await?;
        storage.commit_tx(&mut tx).await?;
        let mut read_tx = begin_tx(2, vec![]);
        assert_eq!(
            storage.get(oid, &i32_bytes(1), &mut read_tx).await?,
            Some(i32_bytes(10))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_reads_own_writes() {
        block_on(async move {
            let r = _worker_storage_reads_own_writes().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_reads_own_writes() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut tx = begin_tx(10, vec![]);

        storage
            .put(oid, i32_bytes(1), i32_bytes(11), &mut tx)
            .await?;

        assert_eq!(
            storage.get(oid, &i32_bytes(1), &mut tx).await?,
            Some(i32_bytes(11))
        );

        let contain_key = storage
            .contains_key(oid, &KeyTuple::from(i32_bytes(1)), &mut tx)
            .await?;
        assert!(contain_key);
        Ok(())
    }

    #[test]
    fn worker_storage_snapshot_hides_later_commit() {
        block_on(async move {
            let r = _worker_storage_snapshot_hides_later_commit().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_snapshot_hides_later_commit() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut tx1 = begin_tx(1, vec![]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(10), &mut tx1)
            .await?;
        storage.commit_tx(&mut tx1).await?;

        let mut old_tx = begin_tx(2, vec![]);
        let mut new_tx = begin_tx(3, vec![2]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(20), &mut new_tx)
            .await?;
        storage.commit_tx(&mut new_tx).await?;

        assert_eq!(
            storage.get(oid, &i32_bytes(1), &mut old_tx).await?,
            Some(i32_bytes(10))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_range_is_stable_with_snapshot() {
        block_on(async move {
            let r = _worker_storage_range_is_stable_with_snapshot().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_range_is_stable_with_snapshot() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut seed = begin_tx(1, vec![]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(10), &mut seed)
            .await?;
        storage.commit_tx(&mut seed).await?;

        let mut old_tx = begin_tx(2, vec![]);
        let mut new_tx = begin_tx(3, vec![2]);
        storage
            .put(oid, i32_bytes(2), i32_bytes(20), &mut new_tx)
            .await?;
        storage.commit_tx(&mut new_tx).await?;

        let rows = storage
            .range(
                oid,
                (
                    Included(i32_bytes(1).as_slice()),
                    Included(i32_bytes(9).as_slice()),
                ),
                &mut old_tx,
            )
            .await?;
        assert_eq!(rows, vec![(i32_bytes(1), i32_bytes(10))]);
        Ok(())
    }

    #[test]
    fn worker_storage_first_committer_wins() {
        block_on(async move {
            let r = _worker_storage_first_committer_wins().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_first_committer_wins() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut seed = begin_tx(1, vec![]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(10), &mut seed)
            .await?;
        storage.commit_tx(&mut seed).await?;

        let mut tx1 = begin_tx(2, vec![]);
        let mut tx2 = begin_tx(3, vec![2]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(11), &mut tx1)
            .await?;
        storage
            .put(oid, i32_bytes(1), i32_bytes(12), &mut tx2)
            .await?;
        storage.commit_tx(&mut tx1).await?;
        let err = storage.commit_tx(&mut tx2).await.unwrap_err();

        assert!(err.to_string().contains("write-write conflict"));
        Ok(())
    }

    #[test]
    fn worker_storage_delete_respects_snapshot() {
        block_on(async move {
            let r = _worker_storage_delete_respects_snapshot().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_delete_respects_snapshot() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut seed = begin_tx(1, vec![]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(10), &mut seed)
            .await?;
        storage.commit_tx(&mut seed).await?;

        let mut old_tx = begin_tx(2, vec![]);
        let mut delete_tx = begin_tx(3, vec![2]);
        assert_eq!(
            storage.remove(oid, &i32_bytes(1), &mut delete_tx).await?,
            Some(i32_bytes(10))
        );
        storage.commit_tx(&mut delete_tx).await?;

        assert_eq!(
            storage.get(oid, &i32_bytes(1), &mut old_tx).await?,
            Some(i32_bytes(10))
        );
        let mut fresh_tx = begin_tx(4, vec![]);
        assert_eq!(storage.get(oid, &i32_bytes(1), &mut fresh_tx).await?, None);
        Ok(())
    }

    #[test]
    fn worker_storage_kv_snapshot_hides_later_commit() {
        block_on(async move {
            let r = _worker_storage_kv_snapshot_hides_later_commit().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_kv_snapshot_hides_later_commit() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        storage.worker_put_local(b"a".to_vec(), b"0".to_vec(), 1)?;

        let snapshot = WorkerSnapshot::new(2, vec![]);
        let prepared = storage.prepare_worker_kv_autocommit(
            3,
            b"a".to_vec(),
            Some(b"1".to_vec()),
            XLBatch::new(vec![]),
        );
        storage.apply_prepared_commit_async(prepared).await?;

        assert_eq!(
            storage.kv_get(b"a", Some(&snapshot)).await?,
            Some(b"0".to_vec())
        );
        assert_eq!(storage.kv_get(b"a", None).await?, Some(b"1".to_vec()));
        Ok(())
    }

    #[test]
    fn worker_storage_kv_range_is_stable_with_snapshot() {
        block_on(async move {
            let r = _worker_storage_kv_range_is_stable_with_snapshot().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_kv_range_is_stable_with_snapshot() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        storage.worker_put_local(b"a".to_vec(), b"1".to_vec(), 1)?;
        let snapshot = WorkerSnapshot::new(2, vec![]);
        storage.worker_put_local(b"b".to_vec(), b"2".to_vec(), 3)?;

        let rows = storage.kv_range(b"a", b"z", Some(&snapshot)).await?;
        assert_eq!(
            rows,
            vec![KvItem {
                key: b"a".to_vec(),
                value: b"1".to_vec()
            }]
        );
        Ok(())
    }

    #[test]
    fn worker_storage_kv_allows_concurrent_commits_on_different_keys() {
        block_on(async move {
            let r = _worker_storage_kv_allows_concurrent_commits_on_different_keys().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_kv_allows_concurrent_commits_on_different_keys() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        let snapshot1 = WorkerSnapshot::new(1, vec![]);
        let snapshot2 = WorkerSnapshot::new(2, vec![1]);

        let prepared1 = storage
            .prepare_worker_kv_commit(
                &snapshot1,
                snapshot1.xid(),
                BTreeMap::from([(b"a".to_vec(), Some(b"1".to_vec()))]),
                XLBatch::new(vec![]),
            )
            .await?;
        let prepared2 = storage
            .prepare_worker_kv_commit(
                &snapshot2,
                snapshot2.xid(),
                BTreeMap::from([(b"b".to_vec(), Some(b"2".to_vec()))]),
                XLBatch::new(vec![]),
            )
            .await?;

        storage.apply_prepared_commit_async(prepared1).await?;
        storage.apply_prepared_commit_async(prepared2).await?;

        assert_eq!(storage.kv_get(b"a", None).await?, Some(b"1".to_vec()));
        assert_eq!(storage.kv_get(b"b", None).await?, Some(b"2".to_vec()));
        Ok(())
    }

    #[test]
    fn worker_storage_replay_batch_restores_kv_and_relation_rows() {
        block_on(async move {
            let r = _worker_storage_replay_batch_restores_kv_and_relation_rows().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_replay_batch_restores_kv_and_relation_rows() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 9,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k".to_vec(),
                    value: b"v".to_vec(),
                })),
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: oid,
                    partition_id: 0,
                    tuple_id: 0,
                    key: i32_bytes(7),
                    value: i32_bytes(70),
                })),
                TxOp::Commit,
            ],
        }]);

        storage.replay_batch(batch).await?;

        assert_eq!(storage.kv_get(b"k", None).await?, Some(b"v".to_vec()));
        let mut tx = begin_tx(10, vec![]);
        assert_eq!(
            storage.get(oid, &i32_bytes(7), &mut tx).await?,
            Some(i32_bytes(70))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_replay_batch_applies_kv_delete() {
        block_on(async move {
            let r = _worker_storage_replay_batch_applies_kv_delete().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_replay_batch_applies_kv_delete() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        storage.worker_put_local(b"k".to_vec(), b"v".to_vec(), 1)?;

        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 2,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Delete(XLDelete {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k".to_vec(),
                })),
                TxOp::Commit,
            ],
        }]);

        storage.replay_batch(batch).await?;

        assert_eq!(storage.kv_get(b"k", None).await?, None);
        Ok(())
    }

    #[test]
    fn worker_storage_cross_partition_apply_is_idempotent() {
        block_on(async move {
            let r = _worker_storage_cross_partition_apply_is_idempotent().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_cross_partition_apply_is_idempotent() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let write = XLWrite::Insert(XLInsert {
            table_id: oid,
            partition_id: 0,
            tuple_id: 0,
            key: i32_bytes(9),
            value: i32_bytes(90),
        });

        storage
            .apply_cross_partition_tx_async(77, &[write.clone()])
            .await?;
        storage.apply_cross_partition_tx_async(77, &[write]).await?;

        let mut tx = begin_tx(78, vec![]);
        assert_eq!(
            storage.get(oid, &i32_bytes(9), &mut tx).await?,
            Some(i32_bytes(90))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables() {
        block_on(async move {
            let r = _worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables() -> RS<()> {
        let mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let oid = schema.id();
        mgr.create_table(&schema).await?;

        let storage = WorkerStorage::new(
            mgr,
            123,
            mudu_sys::env_var::temp_dir()
                .join(format!(
                    "worker_storage_bootstrap_test_{}",
                    mudu_utils::oid::gen_oid()
                ))
                .to_string_lossy()
                .to_string(),
        );
        storage.bootstrap_existing_tables_async().await?;

        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 11,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: oid,
                    partition_id: 0,
                    tuple_id: 0,
                    key: i32_bytes(5),
                    value: i32_bytes(50),
                })),
                TxOp::Commit,
            ],
        }]);

        storage.replay_batch(batch).await?;

        let mut tx = begin_tx(12, vec![]);
        assert_eq!(
            storage
                .get_on_partition(oid, Some(0), &i32_bytes(5), &mut tx)
                .await?,
            Some(i32_bytes(50))
        );
        Ok(())
    }
}
