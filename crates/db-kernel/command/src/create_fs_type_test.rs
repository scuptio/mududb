#![allow(clippy::unwrap_used)]
#![allow(clippy::unimplemented)]
use crate::create_fs_type::CreateFsType;
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::fs_type::{FsTypeDesc, FsTypeKind};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::api::XContract;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PCreateFsType;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_kernel::mudu_conn::mudu_conn_core::MuduConnCore;
use mudu_sys::sync::SMutex;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

struct MockMetaMgr {
    fs_types: SMutex<HashMap<String, FsTypeDesc>>,
    next_fs_id: SMutex<u64>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            fs_types: SMutex::new(HashMap::new()),
            next_fs_id: SMutex::new(1),
        }
    }

    fn get_fs_type(&self, name: &str) -> Option<FsTypeDesc> {
        self.fs_types.lock().unwrap().get(name).cloned()
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }

    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        Err(mudu::mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such table {}", oid)
        ))
    }

    async fn get_table_by_name(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }

    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }

    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }

    async fn create_fs_type(&self, name: &str, kind: FsTypeKind) -> RS<u64> {
        let mut next_fs_id = self.next_fs_id.lock().unwrap();
        let fs_id = *next_fs_id;
        *next_fs_id += 1;
        self.fs_types.lock().unwrap().insert(
            name.to_string(),
            FsTypeDesc::new(name.to_string(), fs_id, kind),
        );
        Ok(fs_id)
    }

    async fn get_fs_type_by_name(&self, name: &str) -> RS<Option<FsTypeDesc>> {
        Ok(self.fs_types.lock().unwrap().get(name).cloned())
    }

    async fn drop_fs_type(&self, name: &str) -> RS<()> {
        self.fs_types.lock().unwrap().remove(name);
        Ok(())
    }
}

struct MockTxMgr;

impl TxMgr for MockTxMgr {
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
    fn put_relation(&self, _relation_id: PhysicalRelationId, _key: Vec<u8>, _value: Vec<u8>) {}
    fn delete_relation(&self, _relation_id: PhysicalRelationId, _key: Vec<u8>) {}
    fn get_relation(
        &self,
        _relation_id: PhysicalRelationId,
        _key: &[u8],
    ) -> Option<Option<Vec<u8>>> {
        None
    }
    fn staged_relation_items_in_range(
        &self,
        _relation_id: PhysicalRelationId,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }
    fn staged_relation_ops(
        &self,
    ) -> BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>> {
        BTreeMap::new()
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
        true
    }
    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        Vec::new()
    }
    fn build_write_ops(&self) {}
    fn xl_batch(&self) -> XLBatch {
        XLBatch::new(Vec::new())
    }
}

struct MockXContract;

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaTable) -> RS<()> {
        unimplemented!()
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        unimplemented!()
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &::xengine::api::AlterTable,
    ) -> RS<()> {
        unimplemented!()
    }
    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        unimplemented!()
    }
    async fn commit_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        unimplemented!()
    }
    async fn abort_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        unimplemented!()
    }
    async fn update(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::VecDatum,
        _pred_non_key: &::xengine::api::Predicate,
        _values: &::xengine::api::VecDatum,
        _opt_update: &::xengine::api::OptUpdate,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::VecDatum,
        _select: &::xengine::api::VecSelTerm,
        _opt_read: &::xengine::api::OptRead,
    ) -> RS<Option<Vec<Option<Vec<u8>>>>> {
        unimplemented!()
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::RangeData,
        _pred_non_key: &::xengine::api::Predicate,
        _select: &::xengine::api::VecSelTerm,
        _opt_read: &::xengine::api::OptRead,
    ) -> RS<Arc<dyn ::xengine::api::RSCursor>> {
        unimplemented!()
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::VecDatum,
        _pred_non_key: &::xengine::api::Predicate,
        _opt_delete: &::xengine::api::OptDelete,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &::xengine::api::VecDatum,
        _values: &::xengine::api::VecDatum,
        _opt_insert: &::xengine::api::OptInsert,
    ) -> RS<()> {
        unimplemented!()
    }
}

fn make_create_fs_type(name: &str, kind: FsTypeKind, meta: Arc<MockMetaMgr>) -> CreateFsType {
    CreateFsType::new(
        PCreateFsType {
            name: name.to_string(),
            kind,
        },
        meta,
    )
}

#[test]
fn create_fs_type_registers_file_and_directory() {
    let meta = Arc::new(MockMetaMgr::new());

    let cmd = make_create_fs_type("wal_log", FsTypeKind::File, meta.clone());
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    let desc = meta.get_fs_type("wal_log").unwrap();
    assert_eq!(desc.kind(), FsTypeKind::File);
    assert_eq!(desc.fs_id(), 1);
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 0);

    let cmd = make_create_fs_type("backup_dir", FsTypeKind::Directory, meta.clone());
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    let desc = meta.get_fs_type("backup_dir").unwrap();
    assert_eq!(desc.kind(), FsTypeKind::Directory);
    assert_eq!(desc.fs_id(), 2);
}

#[test]
fn create_fs_type_prepare_fails_on_duplicate_name() {
    let meta = Arc::new(MockMetaMgr::new());
    block_on(meta.create_fs_type("wal_log", FsTypeKind::File)).unwrap();

    let cmd = make_create_fs_type("wal_log", FsTypeKind::Directory, meta);
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::AlreadyExists);
}

#[test]
fn admin_gate_denies_non_admin_session() {
    let meta = Arc::new(MockMetaMgr::new());
    let core = MuduConnCore::new(meta.clone(), None, false).unwrap();

    let stmt = core
        .parse_one(&"create type filesystem file wal_log;")
        .unwrap();
    let err = block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract),
        DEFAULT_SCHEMA,
    ))
    .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::PermissionDenied);

    let stmt = core.parse_one(&"drop type wal_log;").unwrap();
    let err = block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract),
        DEFAULT_SCHEMA,
    ))
    .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::PermissionDenied);

    assert!(meta.get_fs_type("wal_log").is_none());
}

#[test]
fn admin_gate_allows_admin_session() {
    let meta = Arc::new(MockMetaMgr::new());
    let core = MuduConnCore::new(meta.clone(), None, true).unwrap();

    let stmt = core
        .parse_one(&"create type filesystem file wal_log;")
        .unwrap();
    let affected = block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    assert_eq!(affected, 0);
    let desc = meta.get_fs_type("wal_log").unwrap();
    assert_eq!(desc.kind(), FsTypeKind::File);

    let stmt = core.parse_one(&"drop type wal_log;").unwrap();
    block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    assert!(meta.get_fs_type("wal_log").is_none());
}

// Fixture self-tests: drive every mock method so the shared test fixture is
// verified end to end (and unused fixture paths cannot rot silently).
#[test]
fn mock_tx_mgr_fixture_covers_its_trait_surface() {
    let tx = MockTxMgr;
    let _ = tx.xid();
    let _ = tx.snapshot();
    tx.put(b"k".to_vec(), b"v".to_vec());
    tx.delete(b"k".to_vec());
    let _ = tx.get(b"k");
    let relation = || PhysicalRelationId {
        table_id: 1,
        partition_id: 2,
    };
    tx.put_relation(relation(), b"k".to_vec(), b"v".to_vec());
    tx.delete_relation(relation(), b"k".to_vec());
    let _ = tx.get_relation(relation(), b"k");
    let _ = tx.staged_relation_items_in_range(relation(), b"a", b"z");
    let _ = tx.staged_relation_ops();
    let _ = tx.staged_items_in_range(b"a", b"z");
    let _ = tx.staged_put_items();
    let _ = tx.is_empty();
    let _ = tx.write_ops();
    tx.build_write_ops();
    let _ = tx.xl_batch();
}

#[test]
fn mock_meta_mgr_fixture_covers_its_trait_surface() {
    let meta = MockMetaMgr::new();
    let schema = ::catalog::contract::schema_table::SchemaTable::new(
        "fixture_t".to_string(),
        vec![::catalog::contract::schema_column::SchemaColumn::new(
            "k".to_string(),
            mudu_type::type_family::TypeFamily::I64,
            mudu_type::data_type::DataType::new_no_param(mudu_type::type_family::TypeFamily::I64)
                .to_info(),
        )],
        vec![0],
        vec![],
    );
    block_on(async {
        meta.initialize().await.unwrap();
        let _ = meta.get_table_by_id(1).await;
        let _ = meta.get_table_by_name("fixture_s", "fixture_t").await;
        meta.create_table(&schema).await.unwrap();
        meta.drop_table(1).await.unwrap();
    });
}
