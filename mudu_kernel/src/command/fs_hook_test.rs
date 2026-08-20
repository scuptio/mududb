#![allow(clippy::unwrap_used)]
use crate::command::delete_key_value::DeleteKeyValue;
use crate::command::insert_key_value::InsertKeyValue;
use crate::command::update_key_value::UpdateKeyValue;
use crate::contract::cmd_exec::CmdExec;
use crate::contract::fs_type::{FsColumnBinding, FsTypeKind};
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::meta::fs_object::{
    decode_fs_object_key, decode_fs_object_row, decode_fs_oid_datum, encode_fs_oid_datum,
    gen_fs_oid, FsObjectRow, FS_OBJECT_STATE_PENDING, FS_OBJECT_TABLE_ID,
};
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::{
    OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, VecDatum, VecSelTerm,
    XContract,
};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use crate::x_engine::x_param::{PDeleteKeyValue, PInsertKeyValue, PUpdateKeyValue};
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const TEST_FS_ID: u64 = 7;

fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

fn fs_table_desc() -> Arc<TableDesc> {
    let mut photo = SchemaColumn::new(
        "photo".to_string(),
        TypeFamily::U128,
        DataType::new_no_param(TypeFamily::U128).to_info(),
    );
    photo.set_fs_binding(Some(FsColumnBinding::new(TEST_FS_ID, FsTypeKind::File)));
    let schema = SchemaTable::new(
        "t_fs".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I64,
                DataType::new_no_param(TypeFamily::I64).to_info(),
            ),
            photo,
            SchemaColumn::new(
                "note".to_string(),
                TypeFamily::Binary,
                DataTypeInfo::from_text(TypeFamily::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1, 2],
    );
    TableInfo::new(schema).unwrap().table_desc().unwrap()
}

fn plain_table_desc() -> Arc<TableDesc> {
    let schema = SchemaTable::new(
        "t_plain".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I64,
                DataType::new_no_param(TypeFamily::I64).to_info(),
            ),
            SchemaColumn::new(
                "v".to_string(),
                TypeFamily::I64,
                DataType::new_no_param(TypeFamily::I64).to_info(),
            ),
        ],
        vec![0],
        vec![1],
    );
    TableInfo::new(schema).unwrap().table_desc().unwrap()
}

fn i64_datum(v: i64) -> Buf {
    v.to_be_bytes().to_vec()
}

fn fs_relation_id(partition_id: OID) -> PhysicalRelationId {
    PhysicalRelationId {
        table_id: FS_OBJECT_TABLE_ID,
        partition_id,
    }
}

type StagedOps = BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>;

struct RecordingTxMgr {
    staged: SMutex<StagedOps>,
}

impl RecordingTxMgr {
    fn new() -> Self {
        Self {
            staged: SMutex::new(BTreeMap::new()),
        }
    }

    fn staged_ops(&self) -> StagedOps {
        self.staged.lock().unwrap().clone()
    }
}

impl TxMgr for RecordingTxMgr {
    fn xid(&self) -> u64 {
        1
    }
    fn snapshot(&self) -> WorkerSnapshot {
        WorkerSnapshot::new(1, Vec::new())
    }
    fn put(&self, _key: Vec<u8>, _value: Vec<u8>) {}
    fn delete(&self, _key: Vec<u8>) {}
    fn get(&self, _key: &[u8]) -> Option<Option<Vec<u8>>> {
        None
    }
    fn put_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>, value: Vec<u8>) {
        self.staged
            .lock()
            .unwrap()
            .entry(relation_id)
            .or_default()
            .insert(key, Some(value));
    }
    fn delete_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>) {
        self.staged
            .lock()
            .unwrap()
            .entry(relation_id)
            .or_default()
            .insert(key, None);
    }
    fn get_relation(&self, relation_id: PhysicalRelationId, key: &[u8]) -> Option<Option<Vec<u8>>> {
        self.staged
            .lock()
            .unwrap()
            .get(&relation_id)?
            .get(key)
            .cloned()
    }
    fn staged_relation_items_in_range(
        &self,
        _relation_id: PhysicalRelationId,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }
    fn staged_relation_ops(&self) -> StagedOps {
        self.staged.lock().unwrap().clone()
    }
    fn staged_items_in_range(
        &self,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }
    fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
        BTreeMap::new()
    }
    fn is_empty(&self) -> bool {
        self.staged.lock().unwrap().is_empty()
    }
    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        Vec::new()
    }
    fn build_write_ops(&self) {}
    fn xl_batch(&self) -> XLBatch {
        XLBatch::new(Vec::new())
    }
}

struct MockXContract {
    worker_id: OID,
    read_key_row: Option<Vec<Option<Buf>>>,
    update_return: usize,
    delete_return: usize,
    inserted_values: SMutex<Vec<VecDatum>>,
    updated_values: SMutex<Vec<VecDatum>>,
    delete_calls: AtomicU64,
    read_key_calls: AtomicU64,
}

impl MockXContract {
    fn new(worker_id: OID) -> Self {
        Self {
            worker_id,
            read_key_row: None,
            update_return: 1,
            delete_return: 1,
            inserted_values: SMutex::new(Vec::new()),
            updated_values: SMutex::new(Vec::new()),
            delete_calls: AtomicU64::new(0),
            read_key_calls: AtomicU64::new(0),
        }
    }

    fn with_read_key_row(mut self, row: Vec<Option<Buf>>) -> Self {
        self.read_key_row = Some(row);
        self
    }

    fn inserted(&self) -> Vec<VecDatum> {
        self.inserted_values.lock().unwrap().clone()
    }

    fn updated(&self) -> Vec<VecDatum> {
        self.updated_values.lock().unwrap().clone()
    }
}

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        Ok(())
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &crate::x_engine::api::AlterTable,
    ) -> RS<()> {
        Ok(())
    }
    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(RecordingTxMgr::new()))
    }
    async fn commit_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        Ok(())
    }
    async fn abort_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        Ok(())
    }
    async fn update(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        self.updated_values.lock().unwrap().push(values.clone());
        Ok(self.update_return)
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<Buf>>>> {
        self.read_key_calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.read_key_row.clone())
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &RangeData,
        _pred_non_key: &Predicate,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "mock read_range"
        ))
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        self.delete_calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.delete_return)
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &VecDatum,
        values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        self.inserted_values.lock().unwrap().push(values.clone());
        Ok(())
    }
    fn local_worker_id(&self) -> OID {
        self.worker_id
    }
}

struct MockMetaMgr {
    table_desc: Arc<TableDesc>,
    partition_worker: Option<OID>,
}

impl MockMetaMgr {
    fn new(table_desc: Arc<TableDesc>) -> Self {
        Self {
            table_desc,
            partition_worker: None,
        }
    }

    fn with_partition_worker(mut self, worker_id: OID) -> Self {
        self.partition_worker = Some(worker_id);
        self
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        if oid == self.table_desc.id() {
            Ok(self.table_desc.clone())
        } else {
            Err(mudu::mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such table {}", oid)
            ))
        }
    }
    async fn get_table_by_name(&self, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }
    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }
    async fn get_partition_worker(&self, _partition_id: OID) -> RS<Option<OID>> {
        Ok(self.partition_worker)
    }
}

fn datum_of(row: &VecDatum, attr: usize) -> Option<Buf> {
    row.data()
        .iter()
        .find(|(a, _)| *a == attr)
        .map(|(_, d)| d.clone())
}

#[test]
fn insert_assigns_fs_oid_and_stages_pending_row() {
    block_on(async {
        let desc = fs_table_desc();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(MockXContract::new(0));
        let cmd = InsertKeyValue::new(
            PInsertKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                rows: vec![(
                    VecDatum::new(vec![(0, i64_datum(1))]),
                    VecDatum::new(vec![(2, b"note".to_vec())]),
                )],
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc)),
        );
        cmd.run().await.unwrap();

        // The row datum now carries a system assigned fs object id.
        let inserted = x_contract.inserted();
        assert_eq!(inserted.len(), 1);
        let oid_datum = datum_of(&inserted[0], 1).unwrap();
        let oid = decode_fs_oid_datum(&oid_datum).unwrap();
        assert_eq!(oid >> 120, 0xF5);
        // Non fs columns are passed through untouched.
        assert_eq!(datum_of(&inserted[0], 2), Some(b"note".to_vec()));

        // One PENDING `_fs_object` row is staged on partition 0.
        let staged = tx_mgr.staged_ops();
        let entries = staged.get(&fs_relation_id(0)).unwrap();
        assert_eq!(entries.len(), 1);
        let (key, value) = entries.iter().next().unwrap();
        assert_eq!(decode_fs_object_key(key).unwrap(), oid);
        let row = decode_fs_object_row(value.as_ref().unwrap()).unwrap();
        assert_eq!(
            row,
            FsObjectRow {
                fs_id: TEST_FS_ID,
                kind: FsTypeKind::File.as_u32(),
                generation: 0,
                length: 0,
                state: FS_OBJECT_STATE_PENDING,
            }
        );
        assert_eq!(cmd.affected_rows().await.unwrap(), 1);
        assert_eq!(x_contract.read_key_calls.load(Ordering::Relaxed), 0);
    });
}

#[test]
fn insert_rejects_explicit_fs_value() {
    block_on(async {
        let desc = fs_table_desc();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(MockXContract::new(0));
        let cmd = InsertKeyValue::new(
            PInsertKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                rows: vec![(
                    VecDatum::new(vec![(0, i64_datum(1))]),
                    VecDatum::new(vec![(1, encode_fs_oid_datum(gen_fs_oid()))]),
                )],
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc)),
        );
        let err = cmd.run().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        assert!(x_contract.inserted().is_empty());
        assert!(tx_mgr.staged_ops().is_empty());
    });
}

#[test]
fn insert_on_remote_partition_is_rejected() {
    block_on(async {
        let desc = fs_table_desc();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(MockXContract::new(5));
        let cmd = InsertKeyValue::new(
            PInsertKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                rows: vec![(
                    VecDatum::new(vec![(0, i64_datum(1))]),
                    VecDatum::new(vec![(2, b"note".to_vec())]),
                )],
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc).with_partition_worker(9)),
        );
        let err = cmd.run().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        assert!(x_contract.inserted().is_empty());
        assert!(tx_mgr.staged_ops().is_empty());
    });
}

#[test]
fn update_rebinds_fs_column_and_unbinds_old_oid() {
    block_on(async {
        let desc = fs_table_desc();
        let old_oid = gen_fs_oid();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(
            MockXContract::new(0).with_read_key_row(vec![Some(encode_fs_oid_datum(old_oid))]),
        );
        let cmd = UpdateKeyValue::new(
            PUpdateKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                key: VecDatum::new(vec![(0, i64_datum(1))]),
                // Empty datum: the column is touched, the value is system assigned.
                value: VecDatum::new(vec![(1, Vec::new())]),
                delta_assignments: Vec::new(),
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc)),
        );
        cmd.run().await.unwrap();

        // The update payload now carries a freshly assigned fs object id.
        let updated = x_contract.updated();
        assert_eq!(updated.len(), 1);
        let new_oid = decode_fs_oid_datum(&datum_of(&updated[0], 1).unwrap()).unwrap();
        assert_eq!(new_oid >> 120, 0xF5);
        assert_ne!(new_oid, old_oid);

        // The new object is staged PENDING and the old object is deleted.
        let staged = tx_mgr.staged_ops();
        let entries = staged.get(&fs_relation_id(0)).unwrap();
        assert_eq!(entries.len(), 2);
        let new_key = crate::meta::fs_object::encode_fs_object_key(new_oid).unwrap();
        let old_key = crate::meta::fs_object::encode_fs_object_key(old_oid).unwrap();
        let pending = entries.get(&new_key).unwrap().as_ref().unwrap();
        let row = decode_fs_object_row(pending).unwrap();
        assert_eq!(row.fs_id, TEST_FS_ID);
        assert_eq!(row.state, FS_OBJECT_STATE_PENDING);
        assert!(entries.get(&old_key).unwrap().is_none());
        assert_eq!(cmd.affected_rows().await.unwrap(), 1);
    });
}

#[test]
fn update_rejects_explicit_fs_value() {
    block_on(async {
        let desc = fs_table_desc();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(MockXContract::new(0));
        let cmd = UpdateKeyValue::new(
            PUpdateKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                key: VecDatum::new(vec![(0, i64_datum(1))]),
                value: VecDatum::new(vec![(1, encode_fs_oid_datum(gen_fs_oid()))]),
                delta_assignments: Vec::new(),
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc)),
        );
        let err = cmd.run().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        assert!(x_contract.updated().is_empty());
        assert!(tx_mgr.staged_ops().is_empty());
    });
}

#[test]
fn update_not_touching_fs_columns_is_unchanged() {
    block_on(async {
        let desc = fs_table_desc();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(MockXContract::new(0));
        let value = VecDatum::new(vec![(2, b"note".to_vec())]);
        let cmd = UpdateKeyValue::new(
            PUpdateKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                key: VecDatum::new(vec![(0, i64_datum(1))]),
                value: value.clone(),
                delta_assignments: Vec::new(),
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc)),
        );
        cmd.run().await.unwrap();
        // The payload reaches the contract byte-identical, no fs read/staging.
        assert_eq!(x_contract.updated()[0].data(), value.data());
        assert_eq!(x_contract.read_key_calls.load(Ordering::Relaxed), 0);
        assert!(tx_mgr.staged_ops().is_empty());
    });
}

#[test]
fn delete_unbinds_referenced_fs_objects() {
    block_on(async {
        let desc = fs_table_desc();
        let old_oid = gen_fs_oid();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(
            MockXContract::new(0).with_read_key_row(vec![Some(encode_fs_oid_datum(old_oid))]),
        );
        let cmd = DeleteKeyValue::new(
            PDeleteKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                key: VecDatum::new(vec![(0, i64_datum(1))]),
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc)),
        );
        cmd.run().await.unwrap();

        let staged = tx_mgr.staged_ops();
        let entries = staged.get(&fs_relation_id(0)).unwrap();
        assert_eq!(entries.len(), 1);
        let old_key = crate::meta::fs_object::encode_fs_object_key(old_oid).unwrap();
        assert!(entries.get(&old_key).unwrap().is_none());
        assert_eq!(x_contract.delete_calls.load(Ordering::Relaxed), 1);
        assert_eq!(cmd.affected_rows().await.unwrap(), 1);
    });
}

#[test]
fn delete_of_missing_row_stages_nothing() {
    block_on(async {
        let desc = fs_table_desc();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(MockXContract::new(0));
        let cmd = DeleteKeyValue::new(
            PDeleteKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id: desc.id(),
                key: VecDatum::new(vec![(0, i64_datum(1))]),
            },
            x_contract.clone(),
            Arc::new(MockMetaMgr::new(desc)),
        );
        cmd.run().await.unwrap();
        assert!(tx_mgr.staged_ops().is_empty());
        assert_eq!(x_contract.delete_calls.load(Ordering::Relaxed), 1);
    });
}

#[test]
fn plain_table_dml_stages_no_fs_ops() {
    block_on(async {
        let desc = plain_table_desc();
        let table_id = desc.id();
        let tx_mgr = Arc::new(RecordingTxMgr::new());
        let x_contract = Arc::new(MockXContract::new(0));
        let meta_mgr = Arc::new(MockMetaMgr::new(desc));

        let value = VecDatum::new(vec![(1, i64_datum(10))]);
        let insert = InsertKeyValue::new(
            PInsertKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id,
                rows: vec![(VecDatum::new(vec![(0, i64_datum(1))]), value.clone())],
            },
            x_contract.clone(),
            meta_mgr.clone(),
        );
        insert.run().await.unwrap();

        let update = UpdateKeyValue::new(
            PUpdateKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id,
                key: VecDatum::new(vec![(0, i64_datum(1))]),
                value: value.clone(),
                delta_assignments: Vec::new(),
            },
            x_contract.clone(),
            meta_mgr.clone(),
        );
        update.run().await.unwrap();

        let delete = DeleteKeyValue::new(
            PDeleteKeyValue {
                tx_mgr: tx_mgr.clone(),
                table_id,
                key: VecDatum::new(vec![(0, i64_datum(1))]),
            },
            x_contract.clone(),
            meta_mgr,
        );
        delete.run().await.unwrap();

        // Payloads reach the contract byte-identical and no fs work happens.
        assert_eq!(x_contract.inserted()[0].data(), value.data());
        assert_eq!(x_contract.updated()[0].data(), value.data());
        assert_eq!(x_contract.read_key_calls.load(Ordering::Relaxed), 0);
        assert!(tx_mgr.staged_ops().is_empty());
    });
}
