#![allow(clippy::unwrap_used)]
use crate::command::save_to_file::{SaveToFile, SaveToFileParams};
use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::{
    AlterTable, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, VecDatum,
    VecSelTerm, XContract,
};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field::TupleField;
use mudu_sys::contract::async_file::AsyncFile;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::contract::async_mode::AsyncMode;
use mudu_sys::contract::async_net::AsyncNet;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::sync::SMutex;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

fn sample_schema() -> SchemaTable {
    SchemaTable::new(
        "t".to_string(),
        vec![
            SchemaColumn::new(
                "k".to_string(),
                DatTypeID::I32,
                DatType::default_for(DatTypeID::I32).to_info(),
            ),
            SchemaColumn::new(
                "v".to_string(),
                DatTypeID::String,
                DatType::default_for(DatTypeID::String).to_info(),
            ),
        ],
        vec![0],
        vec![1],
    )
}

fn dummy_table_desc() -> Arc<TableDesc> {
    TableInfo::new(sample_schema())
        .unwrap()
        .table_desc()
        .unwrap()
}

fn string_binary(s: &str) -> Vec<u8> {
    mudu_type::dt_function::send_binary(
        &mudu_type::dat_value::DatValue::from_string(s.to_string()),
        &mudu_type::dat_type::DatType::default_for(mudu_type::dat_type_id::DatTypeID::String),
    )
    .unwrap()
}

fn i32_binary(v: i32) -> Vec<u8> {
    mudu_type::dt_function::send_binary(
        &mudu_type::dat_value::DatValue::from_i32(v),
        &mudu_type::dat_type::DatType::default_for(mudu_type::dat_type_id::DatTypeID::I32),
    )
    .unwrap()
}

struct MockNet;

#[async_trait]
impl AsyncNet for MockNet {}

struct CapturingFile {
    written: Arc<SMutex<Vec<u8>>>,
}

#[async_trait]
impl AsyncFile for CapturingFile {
    async fn read_exact_at(&self, _offset: u64, _len: usize) -> RS<Vec<u8>> {
        Ok(vec![])
    }

    async fn write_all_at(&self, _offset: u64, payload: &[u8]) -> RS<()> {
        self.written.lock().unwrap().extend_from_slice(payload);
        Ok(())
    }

    async fn fsync(&self) -> RS<()> {
        Ok(())
    }

    async fn file_len(&self) -> RS<u64> {
        Ok(0)
    }
}

struct CapturingFs {
    written: Arc<SMutex<Vec<u8>>>,
}

impl CapturingFs {
    fn new() -> (Self, Arc<SMutex<Vec<u8>>>) {
        let written = Arc::new(SMutex::new(Vec::new()));
        (
            Self {
                written: written.clone(),
            },
            written,
        )
    }
}

#[async_trait]
impl AsyncFs for CapturingFs {
    async fn open(&self, _path: &Path, _options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
        Ok(Arc::new(CapturingFile {
            written: self.written.clone(),
        }))
    }

    async fn create_dir_all(&self, _path: &Path) -> RS<()> {
        Ok(())
    }

    async fn metadata_len(&self, _path: &Path) -> RS<u64> {
        Ok(0)
    }

    async fn path_exists(&self, _path: &Path) -> RS<bool> {
        Ok(false)
    }

    async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
        Ok(())
    }

    async fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
        Ok(vec![])
    }
}

struct MockIoProvider {
    fs: Arc<CapturingFs>,
}

impl AsyncIoProvider for MockIoProvider {
    fn mode(&self) -> AsyncMode {
        AsyncMode::Tokio
    }

    fn net(&self) -> &dyn AsyncNet {
        &MockNet
    }

    fn fs(&self) -> &dyn AsyncFs {
        self.fs.as_ref()
    }

    fn fs_arc(&self) -> Arc<dyn AsyncFs> {
        self.fs.clone()
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

struct MockMetaMgr;

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }
    async fn get_table_by_id(&self, _oid: OID) -> RS<Arc<TableDesc>> {
        Ok(dummy_table_desc())
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
}

struct VecCursor {
    rows: SMutex<Vec<TupleField>>,
}

#[async_trait]
impl RSCursor for VecCursor {
    async fn next(&self) -> RS<Option<TupleField>> {
        let mut rows = self.rows.lock().unwrap();
        if rows.is_empty() {
            Ok(None)
        } else {
            Ok(Some(rows.remove(0)))
        }
    }
}

struct MockXContract {
    rows: Vec<TupleField>,
}

impl MockXContract {
    fn new(rows: Vec<TupleField>) -> Self {
        Self { rows }
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
        _alter_table: &AlterTable,
    ) -> RS<()> {
        Ok(())
    }
    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(MockTxMgr))
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
        _values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<Buf>>>> {
        Ok(None)
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
        Ok(Arc::new(VecCursor {
            rows: SMutex::new(self.rows.clone()),
        }))
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &VecDatum,
        _values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        Ok(())
    }
}

fn make_params(
    file_path: &str,
    key_indexing: Vec<usize>,
    value_indexing: Vec<usize>,
    rows: Vec<TupleField>,
    fs: Arc<CapturingFs>,
) -> SaveToFileParams {
    SaveToFileParams {
        file_path: file_path.to_string(),
        tx_mgr: Arc::new(MockTxMgr),
        table_id: 1,
        key_indexing,
        value_indexing,
        x_contract: Arc::new(MockXContract::new(rows)),
        meta_mgr: Arc::new(MockMetaMgr),
        async_runtime: Some(Arc::new(MockIoProvider { fs })),
    }
}

fn row(k: i32, v: &str) -> TupleField {
    TupleField::new_nullable(vec![Some(i32_binary(k)), Some(string_binary(v))])
}

#[test]
fn prepare_fails_when_key_indexing_length_mismatches() {
    let (fs, _written) = CapturingFs::new();
    let cmd = SaveToFile::new(make_params(
        "/tmp/t.csv",
        vec![],
        vec![0, 1],
        vec![],
        Arc::new(fs),
    ));
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidArgument);
}

#[test]
fn prepare_fails_when_indexing_has_duplicate_columns() {
    let (fs, _written) = CapturingFs::new();
    let cmd = SaveToFile::new(make_params(
        "/tmp/t.csv",
        vec![0],
        vec![0],
        vec![],
        Arc::new(fs),
    ));
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidArgument);
}

#[test]
fn prepare_succeeds_when_indexing_is_valid() {
    let (fs, _written) = CapturingFs::new();
    let cmd = SaveToFile::new(make_params(
        "/tmp/t.csv",
        vec![0],
        vec![1],
        vec![],
        Arc::new(fs),
    ));
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn save_table_writes_csv_header_and_rows() {
    let (fs, written) = CapturingFs::new();
    let rows = vec![row(1, "alice"), row(2, "bob")];
    let cmd = SaveToFile::new(make_params(
        "/tmp/t.csv",
        vec![0],
        vec![1],
        rows,
        Arc::new(fs),
    ));

    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
    });

    let payload = String::from_utf8(written.lock().unwrap().clone()).unwrap();
    assert!(payload.contains("k"));
    assert!(payload.contains("v"));
    assert!(payload.contains("alice"));
    assert!(payload.contains("bob"));
}

#[test]
fn save_table_propagates_error_when_runtime_missing() {
    let cmd = SaveToFile::new(SaveToFileParams {
        file_path: "/tmp/t.csv".to_string(),
        tx_mgr: Arc::new(MockTxMgr),
        table_id: 1,
        key_indexing: vec![0],
        value_indexing: vec![1],
        x_contract: Arc::new(MockXContract::new(vec![])),
        meta_mgr: Arc::new(MockMetaMgr),
        async_runtime: None,
    });

    block_on(async {
        cmd.prepare().await.unwrap();
        let err = cmd.run().await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidState);
    });
}
