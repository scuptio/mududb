#![allow(clippy::unwrap_used)]
use crate::command::load_from_file::{LoadFromFile, LoadFromFileParams};
use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::{OptInsert, VecDatum, XContract};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
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

struct MockNet;

#[async_trait]
impl AsyncNet for MockNet {}

struct MockFile {
    content: Vec<u8>,
}

#[async_trait]
impl AsyncFile for MockFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let start = offset as usize;
        let end = (start + len).min(self.content.len());
        Ok(self.content[start..end].to_vec())
    }

    async fn write_all_at(&self, _offset: u64, _payload: &[u8]) -> RS<()> {
        Ok(())
    }

    async fn fsync(&self) -> RS<()> {
        Ok(())
    }

    async fn file_len(&self) -> RS<u64> {
        Ok(self.content.len() as u64)
    }
}

struct MockFs {
    files: SMutex<BTreeMap<PathBuf, Vec<u8>>>,
}

impl MockFs {
    fn with_file(path: impl AsRef<Path>, content: Vec<u8>) -> Self {
        let mut files = BTreeMap::new();
        files.insert(path.as_ref().to_path_buf(), content);
        Self {
            files: SMutex::new(files),
        }
    }
}

#[async_trait]
impl AsyncFs for MockFs {
    async fn open(&self, path: &Path, _options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
        let files = self.files.lock().unwrap();
        let content = files.get(path).cloned().unwrap_or_default();
        Ok(Arc::new(MockFile { content }))
    }

    async fn create_dir_all(&self, _path: &Path) -> RS<()> {
        Ok(())
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        let files = self.files.lock().unwrap();
        Ok(files.get(path).map_or(0, |content| content.len() as u64))
    }

    async fn path_exists(&self, path: &Path) -> RS<bool> {
        let files = self.files.lock().unwrap();
        Ok(files.contains_key(path))
    }

    async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
        Ok(())
    }

    async fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
        Ok(vec![])
    }
}

struct MockIoProvider {
    fs: Arc<MockFs>,
}

impl AsyncIoProvider for MockIoProvider {
    fn mode(&self) -> AsyncMode {
        AsyncMode::Tokio
    }

    fn net(&self) -> &dyn AsyncNet {
        // `MockNet` is a zero-sized type, so a reference to a temporary is fine
        // for a provider that never performs network I/O.
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

struct MockXContract {
    inserted: SMutex<Vec<(VecDatum, VecDatum)>>,
}

impl MockXContract {
    fn new() -> Self {
        Self {
            inserted: SMutex::new(Vec::new()),
        }
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
        _pred_non_key: &crate::x_engine::api::Predicate,
        _values: &VecDatum,
        _opt_update: &crate::x_engine::api::OptUpdate,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _select: &crate::x_engine::api::VecSelTerm,
        _opt_read: &crate::x_engine::api::OptRead,
    ) -> RS<Option<Vec<Option<Buf>>>> {
        Ok(None)
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
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "mock read_range"
        ))
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &crate::x_engine::api::Predicate,
        _opt_delete: &crate::x_engine::api::OptDelete,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        self.inserted
            .lock()
            .unwrap()
            .push((keys.clone(), values.clone()));
        Ok(())
    }
}

fn csv_payload(rows: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
    writer.write_record(["k", "v"]).unwrap();
    for (k, v) in rows {
        writer.write_record([*k, *v]).unwrap();
    }
    writer.into_inner().unwrap()
}

fn make_params(
    csv_path: &str,
    key_index: Vec<usize>,
    value_index: Vec<usize>,
    fs: Arc<MockFs>,
) -> LoadFromFileParams {
    LoadFromFileParams {
        csv_file: csv_path.to_string(),
        tx_mgr: Arc::new(MockTxMgr),
        table_id: 1,
        key_index,
        value_index,
        x_contract: Arc::new(MockXContract::new()),
        meta_mgr: Arc::new(MockMetaMgr),
        async_runtime: Some(Arc::new(MockIoProvider { fs })),
    }
}

#[test]
fn prepare_fails_when_key_index_length_mismatches() {
    let fs = Arc::new(MockFs::with_file("/tmp/t.csv", csv_payload(&[("1", "a")])));
    let cmd = LoadFromFile::new(make_params("/tmp/t.csv", vec![], vec![0], fs));
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidArgument);
}

#[test]
fn prepare_succeeds_when_index_lengths_match() {
    let fs = Arc::new(MockFs::with_file("/tmp/t.csv", csv_payload(&[("1", "a")])));
    let cmd = LoadFromFile::new(make_params("/tmp/t.csv", vec![0], vec![1], fs));
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn load_table_parses_csv_rows_and_inserts_them() {
    let fs = Arc::new(MockFs::with_file(
        "/tmp/t.csv",
        csv_payload(&[("1", "alice"), ("2", "bob")]),
    ));
    let x_contract = Arc::new(MockXContract::new());
    let cmd = LoadFromFile::new(LoadFromFileParams {
        csv_file: "/tmp/t.csv".to_string(),
        tx_mgr: Arc::new(MockTxMgr),
        table_id: 1,
        key_index: vec![0],
        value_index: vec![1],
        x_contract: x_contract.clone(),
        meta_mgr: Arc::new(MockMetaMgr),
        async_runtime: Some(Arc::new(MockIoProvider { fs })),
    });

    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
        assert_eq!(cmd.affected_rows().await.unwrap(), 2);
    });

    let inserted = x_contract.inserted.lock().unwrap();
    assert_eq!(inserted.len(), 2);
    assert_eq!(inserted[0].0.data().len(), 1);
    assert_eq!(inserted[0].1.data().len(), 1);
}

#[test]
fn load_table_rejects_csv_column_count_mismatch() {
    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
    writer.write_record(["k", "v", "extra"]).unwrap();
    writer.write_record(["1", "a", "x"]).unwrap();
    let payload = writer.into_inner().unwrap();

    let fs = Arc::new(MockFs::with_file("/tmp/t.csv", payload));
    let cmd = LoadFromFile::new(make_params("/tmp/t.csv", vec![0], vec![1], fs));
    block_on(async {
        cmd.prepare().await.unwrap();
        let err = cmd.run().await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidArgument);
    });
}

#[test]
fn load_table_propagates_open_error_when_runtime_missing() {
    let cmd = LoadFromFile::new(LoadFromFileParams {
        csv_file: "/tmp/t.csv".to_string(),
        tx_mgr: Arc::new(MockTxMgr),
        table_id: 1,
        key_index: vec![0],
        value_index: vec![1],
        x_contract: Arc::new(MockXContract::new()),
        meta_mgr: Arc::new(MockMetaMgr),
        async_runtime: None,
    });

    block_on(async {
        cmd.prepare().await.unwrap();
        let err = cmd.run().await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidState);
    });
}

#[test]
fn load_table_accepts_quoted_file_path() {
    let fs = Arc::new(MockFs::with_file(
        "/tmp/t.csv",
        csv_payload(&[("7", "seven")]),
    ));
    let x_contract = Arc::new(MockXContract::new());
    let cmd = LoadFromFile::new(LoadFromFileParams {
        csv_file: "'/tmp/t.csv'".to_string(),
        tx_mgr: Arc::new(MockTxMgr),
        table_id: 1,
        key_index: vec![0],
        value_index: vec![1],
        x_contract: x_contract.clone(),
        meta_mgr: Arc::new(MockMetaMgr),
        async_runtime: Some(Arc::new(MockIoProvider { fs })),
    });

    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
        assert_eq!(cmd.affected_rows().await.unwrap(), 1);
    });

    assert_eq!(x_contract.inserted.lock().unwrap().len(), 1);
}
