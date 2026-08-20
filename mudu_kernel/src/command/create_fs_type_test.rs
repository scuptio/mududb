#![allow(clippy::unwrap_used)]
#![allow(clippy::unimplemented)]
use crate::command::create_fs_type::CreateFsType;
use crate::contract::cmd_exec::CmdExec;
use crate::contract::fs_type::{FsTypeDesc, FsTypeKind};
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::mudu_conn::mudu_conn_core::MuduConnCore;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::XContract;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use crate::x_engine::x_param::PCreateFsType;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
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

    async fn get_table_by_name(&self, _name: &str) -> RS<Option<Arc<TableDesc>>> {
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
        _alter_table: &crate::x_engine::api::AlterTable,
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
        _pred_key: &crate::x_engine::api::VecDatum,
        _pred_non_key: &crate::x_engine::api::Predicate,
        _values: &crate::x_engine::api::VecDatum,
        _opt_update: &crate::x_engine::api::OptUpdate,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &crate::x_engine::api::VecDatum,
        _select: &crate::x_engine::api::VecSelTerm,
        _opt_read: &crate::x_engine::api::OptRead,
    ) -> RS<Option<Vec<Option<Vec<u8>>>>> {
        unimplemented!()
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &crate::x_engine::api::RangeData,
        _pred_non_key: &crate::x_engine::api::Predicate,
        _select: &crate::x_engine::api::VecSelTerm,
        _opt_read: &crate::x_engine::api::OptRead,
    ) -> RS<Arc<dyn crate::x_engine::api::RSCursor>> {
        unimplemented!()
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &crate::x_engine::api::VecDatum,
        _pred_non_key: &crate::x_engine::api::Predicate,
        _opt_delete: &crate::x_engine::api::OptDelete,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &crate::x_engine::api::VecDatum,
        _values: &crate::x_engine::api::VecDatum,
        _opt_insert: &crate::x_engine::api::OptInsert,
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
    ))
    .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::PermissionDenied);

    let stmt = core.parse_one(&"drop type wal_log;").unwrap();
    let err = block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract),
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
    ))
    .unwrap();
    assert!(meta.get_fs_type("wal_log").is_none());
}
