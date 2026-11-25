#![allow(clippy::unimplemented)]

use super::kernel_async::*;
use super::kernel_sync::*;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_class::SchemaClass;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::worker_snapshot::KvItem;
use ::conn::mudu_conn_async::MuduConnAsync;
use ::conn::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
use ::xengine::DataBin;
use ::xengine::api::{
    AlterTable, DeltaOp, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData,
    VecDatum, VecSelTerm, XContract, XContractQueue,
};
use ::xengine::tx_mgr::TxMgr;
use async_trait::async_trait;
use mudu::channel::{Envelope, MessageId, MessageKind as ChannelKind, OutgoingMessage, RecvFilter};
use mudu::common::buf::Buf;
use mudu::common::id::AttrIndex;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_binding::codec::syscall_payload::{
    MessageKind, decode_close_result, decode_delete_result, decode_fs_close_result,
    decode_fs_fstat_result, decode_fs_fsync_result, decode_fs_lseek_result, decode_fs_open_result,
    decode_fs_pread_result, decode_fs_pwrite_result, decode_fs_read_result,
    decode_fs_readdir_result, decode_fs_stat_result, decode_fs_write_result, decode_get_result,
    decode_open_result, decode_put_result, decode_range_result, decode_relation_get_result,
    decode_relation_insert_result, decode_relation_update_result, encode_close_request,
    encode_delete_request, encode_frame, encode_fs_close_request, encode_fs_fstat_request,
    encode_fs_fsync_request, encode_fs_lseek_request, encode_fs_open_request,
    encode_fs_pread_request, encode_fs_pwrite_request, encode_fs_read_request,
    encode_fs_readdir_request, encode_fs_stat_request, encode_fs_write_request, encode_get_request,
    encode_open_request, encode_put_request, encode_range_request, encode_relation_get_request,
    encode_relation_insert_request, encode_relation_update_request,
};
use mudu_binding::universal::mp_wire::{FromValue, ToValue, decode_value, encode_value};
use mudu_binding::universal::uni_error::UniError;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_result::UniResult;
use mudu_binding::universal::uni_result_set::UniResultSet;
use mudu_contract::database::db_conn::DBConnSync;
use mudu_contract::database::object::Object;
use mudu_contract::database::result_set::{ResultSet, ResultSetAsync};
use mudu_contract::database::sql::{Context, DBConn};
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_value::DataValue;
use mudu_utils::channel::{ChannelEndpoint, KindHandler};
use std::collections::HashMap;
use std::sync::Arc;

use mudu_sys::sync::SMutex;

mod fetch;
mod host_wrappers;
mod kv_fs_frames;
mod relation;
mod session;
mod sql_errors;
mod surface;

struct NullXContract;

#[async_trait]
impl XContract for NullXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaClass) -> RS<()> {
        unimplemented!()
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        unimplemented!()
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &AlterTable,
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
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<Buf>>>> {
        unimplemented!()
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
        unimplemented!()
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &VecDatum,
        _values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        unimplemented!()
    }
}

impl XContractQueue for NullXContract {}

struct NullMetaMgr;

#[async_trait]
impl MetaMgr for NullMetaMgr {
    async fn initialize(&self) -> RS<()> {
        unimplemented!()
    }
    async fn get_table_by_id(&self, _oid: OID) -> RS<Arc<TableDesc>> {
        unimplemented!()
    }
    async fn get_table_by_name(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        unimplemented!()
    }
    async fn create_table(&self, _schema: &SchemaClass) -> RS<()> {
        unimplemented!()
    }
    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        unimplemented!()
    }
    async fn create_partition_rule(&self, _rule: &PartitionRuleDesc) -> RS<()> {
        unimplemented!()
    }
    async fn get_partition_rule_by_id(&self, _oid: OID) -> RS<PartitionRuleDesc> {
        unimplemented!()
    }
    async fn get_partition_rule_by_name(&self, _name: &str) -> RS<Option<PartitionRuleDesc>> {
        unimplemented!()
    }
    async fn list_partition_rules(&self) -> RS<Vec<PartitionRuleDesc>> {
        unimplemented!()
    }
    async fn bind_table_partition(&self, _binding: &TablePartitionBinding) -> RS<()> {
        unimplemented!()
    }
    async fn get_table_partition_binding(
        &self,
        _table_id: OID,
    ) -> RS<Option<TablePartitionBinding>> {
        unimplemented!()
    }
    async fn upsert_partition_placements(&self, _placements: &[PartitionPlacement]) -> RS<()> {
        unimplemented!()
    }
    async fn get_partition_worker(&self, _partition_id: OID) -> RS<Option<OID>> {
        unimplemented!()
    }
    async fn list_partition_placements(&self) -> RS<Vec<PartitionPlacement>> {
        unimplemented!()
    }
    async fn list_schemas(&self) -> RS<Vec<SchemaClass>> {
        unimplemented!()
    }
}

struct NullWorkerChannel;

#[async_trait]
impl ChannelEndpoint for NullWorkerChannel {
    fn local_endpoint(&self) -> OID {
        unimplemented!()
    }
    async fn send(&self, _dst: OID, _message: OutgoingMessage) -> RS<MessageId> {
        unimplemented!()
    }
    async fn recv(&self, _filter: RecvFilter, _deadline_ms: Option<u64>) -> RS<Envelope> {
        unimplemented!()
    }
    fn register_kind_handler(&self, _kind: ChannelKind, _handler: KindHandler) -> RS<()> {
        unimplemented!()
    }
    fn unregister_kind_handler(&self, _kind: ChannelKind) -> RS<()> {
        unimplemented!()
    }
}

pub(crate) struct FakeWorkerLocal {
    next_id: SMutex<u128>,
    store: SMutex<HashMap<Vec<u8>, Vec<u8>>>,
}

impl FakeWorkerLocal {
    fn new() -> Self {
        Self {
            next_id: SMutex::new(1),
            store: SMutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl WorkerLocal for FakeWorkerLocal {
    fn x_contract(&self) -> Arc<dyn XContract> {
        Arc::new(NullXContract)
    }
    fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        Arc::new(NullMetaMgr)
    }
    fn worker_channel(&self) -> Arc<dyn ChannelEndpoint> {
        Arc::new(NullWorkerChannel)
    }
    async fn open_async(&self) -> RS<OID> {
        let mut id = self.next_id.lock().unwrap();
        let session_id = *id;
        *id += 1;
        Ok(session_id)
    }
    async fn open_argv_async(&self, worker_id: OID) -> RS<OID> {
        if worker_id == 0 {
            self.open_async().await
        } else {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::NotImplemented,
                "worker-local open not supported"
            ))
        }
    }
    async fn close_async(&self, _session_id: OID) -> RS<()> {
        Ok(())
    }
    async fn execute_async(&self, _session_id: OID, _instruction: WorkerExecute) -> RS<()> {
        unimplemented!()
    }
    async fn put_async(&self, _session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        self.store.lock().unwrap().insert(key, value);
        Ok(())
    }
    async fn delete_async(&self, _session_id: OID, key: &[u8]) -> RS<()> {
        self.store.lock().unwrap().remove(key);
        Ok(())
    }
    async fn get_async(&self, _session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }
    async fn range_async(
        &self,
        _session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        let store = self.store.lock().unwrap();
        let mut items: Vec<KvItem> = store
            .iter()
            .filter(|(k, _)| k.as_slice() >= start_key && k.as_slice() < end_key)
            .map(|(k, v)| KvItem {
                key: k.clone(),
                value: v.clone(),
            })
            .collect();
        items.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(items)
    }
    async fn query(
        &self,
        _oid: OID,
        _app_name: Option<&str>,
        _sql: Box<dyn SQLStmt>,
        _param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        unimplemented!()
    }
    async fn execute(
        &self,
        _oid: OID,
        _app_name: Option<&str>,
        _sql: Box<dyn SQLStmt>,
        _param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        unimplemented!()
    }
    async fn batch(
        &self,
        _oid: OID,
        _app_name: Option<&str>,
        _sql: Box<dyn SQLStmt>,
        _param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        unimplemented!()
    }
}

pub(crate) fn worker_local() -> WorkerLocalRef {
    Arc::new(FakeWorkerLocal::new())
}

/// A worker-local that only serves sessions it minted itself:
/// `open_async` issues and registers real session ids, and every
/// session-bound operation rejects unknown ids — the same contract
/// `WorkerSessionManager` enforces. A syscall that forwards the
/// guest-side task id (instead of resolving the task context's
/// connection session) fails here, reproducing the pre-fix KV behavior.
pub(crate) struct SessionAwareWorkerLocal {
    base: FakeWorkerLocal,
    live_sessions: SMutex<std::collections::HashSet<OID>>,
}

impl SessionAwareWorkerLocal {
    fn new() -> Self {
        Self {
            base: FakeWorkerLocal::new(),
            live_sessions: SMutex::new(std::collections::HashSet::new()),
        }
    }

    fn live_session_count(&self) -> usize {
        self.live_sessions.lock().unwrap().len()
    }

    fn require_live(&self, session_id: OID) -> RS<()> {
        if self.live_sessions.lock().unwrap().contains(&session_id) {
            Ok(())
        } else {
            Err(mudu::mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("session {} does not exist", session_id)
            ))
        }
    }
}

#[async_trait]
impl WorkerLocal for SessionAwareWorkerLocal {
    fn x_contract(&self) -> Arc<dyn XContract> {
        self.base.x_contract()
    }
    fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.base.meta_mgr()
    }
    fn worker_channel(&self) -> Arc<dyn ChannelEndpoint> {
        self.base.worker_channel()
    }
    async fn open_async(&self) -> RS<OID> {
        let session_id = self.base.open_async().await?;
        self.live_sessions.lock().unwrap().insert(session_id);
        Ok(session_id)
    }
    async fn close_async(&self, session_id: OID) -> RS<()> {
        self.require_live(session_id)?;
        self.live_sessions.lock().unwrap().remove(&session_id);
        Ok(())
    }
    async fn execute_async(&self, session_id: OID, instruction: WorkerExecute) -> RS<()> {
        self.require_live(session_id)?;
        self.base.execute_async(session_id, instruction).await
    }
    async fn put_async(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        self.require_live(session_id)?;
        self.base.put_async(session_id, key, value).await
    }
    async fn delete_async(&self, session_id: OID, key: &[u8]) -> RS<()> {
        self.require_live(session_id)?;
        self.base.delete_async(session_id, key).await
    }
    async fn get_async(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.require_live(session_id)?;
        self.base.get_async(session_id, key).await
    }
    async fn range_async(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.require_live(session_id)?;
        self.base.range_async(session_id, start_key, end_key).await
    }
    async fn query(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        self.base.query(oid, app_name, sql, param).await
    }
    async fn execute(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        self.base.execute(oid, app_name, sql, param).await
    }
    async fn batch(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        self.base.batch(oid, app_name, sql, param).await
    }
}

pub(crate) fn session_aware_worker_local() -> Arc<SessionAwareWorkerLocal> {
    Arc::new(SessionAwareWorkerLocal::new())
}

/// A worker-local with a minimal in-memory relation store, used to verify
/// the relation syscall plumbing (frame decode, attribute/delta mapping).
type RelationRows = HashMap<(String, Vec<u8>), Vec<(AttrIndex, DataBin)>>;

pub(crate) struct RelationWorkerLocal {
    base: FakeWorkerLocal,
    rows: SMutex<RelationRows>,
    last_deltas: SMutex<Vec<(AttrIndex, DeltaOp, DataBin)>>,
}

impl RelationWorkerLocal {
    fn new() -> Self {
        Self {
            base: FakeWorkerLocal::new(),
            rows: SMutex::new(HashMap::new()),
            last_deltas: SMutex::new(Vec::new()),
        }
    }
}

fn relation_key_bytes(table: &str, key: &[(AttrIndex, DataBin)]) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    for (attr, datum) in key {
        bytes.extend_from_slice(&attr.to_be_bytes());
        bytes.extend_from_slice(datum);
    }
    (table.to_string(), bytes)
}

#[async_trait]
impl WorkerLocal for RelationWorkerLocal {
    fn x_contract(&self) -> Arc<dyn XContract> {
        self.base.x_contract()
    }
    fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.base.meta_mgr()
    }
    fn worker_channel(&self) -> Arc<dyn ChannelEndpoint> {
        self.base.worker_channel()
    }
    async fn open_async(&self) -> RS<OID> {
        self.base.open_async().await
    }
    async fn close_async(&self, session_id: OID) -> RS<()> {
        self.base.close_async(session_id).await
    }
    async fn execute_async(&self, session_id: OID, instruction: WorkerExecute) -> RS<()> {
        self.base.execute_async(session_id, instruction).await
    }
    async fn put_async(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        self.base.put_async(session_id, key, value).await
    }
    async fn delete_async(&self, session_id: OID, key: &[u8]) -> RS<()> {
        self.base.delete_async(session_id, key).await
    }
    async fn get_async(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.base.get_async(session_id, key).await
    }
    async fn range_async(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.base.range_async(session_id, start_key, end_key).await
    }
    async fn query(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        self.base.query(oid, app_name, sql, param).await
    }
    async fn execute(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        self.base.execute(oid, app_name, sql, param).await
    }
    async fn batch(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        self.base.batch(oid, app_name, sql, param).await
    }
    async fn relation_get(
        &self,
        _session_id: OID,
        _app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let rows = self.rows.lock().unwrap();
        let Some(row) = rows.get(&relation_key_bytes(table, &key)) else {
            return Ok(None);
        };
        Ok(Some(
            select
                .iter()
                .map(|attr| {
                    row.iter()
                        .find(|(a, _)| a == attr)
                        .map(|(_, datum)| datum.clone())
                })
                .collect(),
        ))
    }
    async fn relation_update(
        &self,
        _session_id: OID,
        _app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
        deltas: Vec<(AttrIndex, DeltaOp, DataBin)>,
    ) -> RS<u64> {
        let mut rows = self.rows.lock().unwrap();
        let Some(row) = rows.get_mut(&relation_key_bytes(table, &key)) else {
            return Ok(0);
        };
        for (attr, datum) in values {
            match row.iter_mut().find(|(a, _)| *a == attr) {
                Some(slot) => slot.1 = datum,
                None => row.push((attr, datum)),
            }
        }
        *self.last_deltas.lock().unwrap() = deltas;
        Ok(1)
    }
    async fn relation_insert(
        &self,
        _session_id: OID,
        _app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<()> {
        let mut rows = self.rows.lock().unwrap();
        let id = relation_key_bytes(table, &key);
        if rows.contains_key(&id) {
            return Err(mudu::mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                "existing key"
            ));
        }
        let mut row = key;
        row.extend(values);
        rows.insert(id, row);
        Ok(())
    }
}

pub(crate) fn relation_worker_local() -> WorkerLocalRef {
    Arc::new(RelationWorkerLocal::new())
}

/// Register a task context whose async connection is bound to `wl` — the
/// same binding `app_inst_impl` creates for a procedure invocation.
/// Relation syscalls resolve the connection (and with it the session)
/// through this context; the frame's session field is only the lookup
/// key.
pub(crate) fn relation_context(oid: OID, wl: WorkerLocalRef) {
    let conn = MuduConnAsync::new_with_worker_local(wl).unwrap();
    Context::create(oid, DBConn::Async(Arc::new(conn))).unwrap();
}

// ---------------------------------------------------------------------------
// In-memory fs service + a worker-local exposing it (fs syscall tests)
// ---------------------------------------------------------------------------

/// Content filesystem mock: a global fd table over byte buffers with a
/// cursor per fd; `/missing` is the only absent path.
pub(crate) struct MockFsService {
    files: SMutex<HashMap<u32, (Vec<u8>, u64)>>,
    next_fd: SMutex<u32>,
}

impl MockFsService {
    fn new() -> Self {
        Self {
            files: SMutex::new(HashMap::new()),
            next_fd: SMutex::new(3),
        }
    }

    fn entry(&self, fd: u32) -> RS<(Vec<u8>, u64)> {
        self.files
            .lock()
            .unwrap()
            .get(&fd)
            .cloned()
            .ok_or_else(|| mudu::mudu_error!(ErrorCode::InvalidArgument, "bad fs fd"))
    }
}

#[async_trait]
impl ::conn::worker_local::WorkerFsService for MockFsService {
    async fn fs_open(&self, _session_id: OID, oid: OID, path: &str, _flags: u32) -> RS<u32> {
        if path == "/missing" {
            return Err(mudu::mudu_error!(
                ErrorCode::ObjectNotFound,
                "no such fs object"
            ));
        }
        let mut files = self.files.lock().unwrap();
        let mut next = self.next_fd.lock().unwrap();
        let fd = *next;
        *next += 1;
        files.insert(fd, (b"hello-fs".to_vec(), 0));
        let _ = oid;
        Ok(fd)
    }

    async fn fs_close(&self, _session_id: OID, fd: u32) -> RS<()> {
        self.files
            .lock()
            .unwrap()
            .remove(&fd)
            .map(|_| ())
            .ok_or_else(|| mudu::mudu_error!(ErrorCode::InvalidArgument, "bad fs fd"))
    }

    async fn fs_read(&self, _session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
        let mut files = self.files.lock().unwrap();
        let (content, cursor) = files
            .get_mut(&fd)
            .ok_or_else(|| mudu::mudu_error!(ErrorCode::InvalidArgument, "bad fs fd"))?;
        let start = (*cursor).min(content.len() as u64) as usize;
        let end = (start + len as usize).min(content.len());
        let out = content[start..end].to_vec();
        *cursor = end as u64;
        Ok(out)
    }

    async fn fs_write(&self, _session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
        let mut files = self.files.lock().unwrap();
        let (content, cursor) = files
            .get_mut(&fd)
            .ok_or_else(|| mudu::mudu_error!(ErrorCode::InvalidArgument, "bad fs fd"))?;
        let at = *cursor as usize;
        if content.len() < at + data.len() {
            content.resize(at + data.len(), 0);
        }
        content[at..at + data.len()].copy_from_slice(data);
        *cursor = (at + data.len()) as u64;
        Ok(data.len() as u32)
    }

    async fn fs_pread(&self, _session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
        let (content, _) = self.entry(fd)?;
        let start = (offset as usize).min(content.len());
        let end = (start + len as usize).min(content.len());
        Ok(content[start..end].to_vec())
    }

    async fn fs_pwrite(&self, _session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
        let mut files = self.files.lock().unwrap();
        let (content, _) = files
            .get_mut(&fd)
            .ok_or_else(|| mudu::mudu_error!(ErrorCode::InvalidArgument, "bad fs fd"))?;
        let at = offset as usize;
        if content.len() < at + data.len() {
            content.resize(at + data.len(), 0);
        }
        content[at..at + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn fs_lseek(&self, _session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
        let mut files = self.files.lock().unwrap();
        let (content, cursor) = files
            .get_mut(&fd)
            .ok_or_else(|| mudu::mudu_error!(ErrorCode::InvalidArgument, "bad fs fd"))?;
        let base = match whence {
            0 => 0i64,
            1 => *cursor as i64,
            2 => content.len() as i64,
            _ => return Err(mudu::mudu_error!(ErrorCode::InvalidArgument, "bad whence")),
        };
        let next = base + offset;
        if next < 0 {
            return Err(mudu::mudu_error!(
                ErrorCode::InvalidArgument,
                "negative seek"
            ));
        }
        *cursor = next as u64;
        Ok(*cursor)
    }

    fn fs_fstat(&self, _session_id: OID, fd: u32) -> RS<UniFsStat> {
        let (content, _) = self.entry(fd)?;
        Ok(UniFsStat {
            oid: UniOid::from_oid(2),
            generation: 1,
            entry: "data.bin".to_string(),
            length: content.len() as u64,
            state: 1,
        })
    }

    async fn fs_fsync(&self, _session_id: OID, fd: u32) -> RS<()> {
        let _ = self.entry(fd)?;
        Ok(())
    }

    async fn fs_stat(&self, _session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
        if path == "/missing" {
            return Err(mudu::mudu_error!(
                ErrorCode::ObjectNotFound,
                "no such fs object"
            ));
        }
        Ok(UniFsStat {
            oid: UniOid::from_oid(oid),
            generation: 1,
            entry: path.to_string(),
            length: 8,
            state: 1,
        })
    }

    async fn fs_readdir(&self, _session_id: OID, _oid: OID, _path: &str) -> RS<Vec<UniFsDirent>> {
        Ok(vec![UniFsDirent {
            name: "data.bin".to_string(),
            is_dir: false,
            length: 8,
        }])
    }
}

/// A worker-local that exposes the in-memory fs service and reports a fixed
/// current session (the fs syscalls resolve the session through
/// `current_session_id`, opens through `open_async`).
pub(crate) struct FsWorkerLocal {
    base: FakeWorkerLocal,
    session_id: OID,
    fs: Arc<MockFsService>,
}

#[async_trait]
impl WorkerLocal for FsWorkerLocal {
    fn x_contract(&self) -> Arc<dyn XContract> {
        self.base.x_contract()
    }
    fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.base.meta_mgr()
    }
    fn worker_channel(&self) -> Arc<dyn ChannelEndpoint> {
        self.base.worker_channel()
    }
    fn fs_service(&self) -> RS<Arc<dyn ::conn::worker_local::WorkerFsService>> {
        Ok(self.fs.clone())
    }
    fn current_session_id(&self) -> Option<OID> {
        Some(self.session_id)
    }
    async fn open_async(&self) -> RS<OID> {
        Ok(self.session_id)
    }
    async fn open_argv_async(&self, _worker_id: OID) -> RS<OID> {
        Ok(self.session_id)
    }
    async fn close_async(&self, session_id: OID) -> RS<()> {
        self.base.close_async(session_id).await
    }
    async fn execute_async(&self, session_id: OID, instruction: WorkerExecute) -> RS<()> {
        self.base.execute_async(session_id, instruction).await
    }
    async fn put_async(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        self.base.put_async(session_id, key, value).await
    }
    async fn delete_async(&self, session_id: OID, key: &[u8]) -> RS<()> {
        self.base.delete_async(session_id, key).await
    }
    async fn get_async(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.base.get_async(session_id, key).await
    }
    async fn range_async(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.base.range_async(session_id, start_key, end_key).await
    }
    async fn query(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        self.base.query(oid, app_name, sql, param).await
    }
    async fn execute(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        self.base.execute(oid, app_name, sql, param).await
    }
    async fn batch(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        self.base.batch(oid, app_name, sql, param).await
    }
}

/// An fs-serving worker-local bound to session 1.
pub(crate) fn fs_worker_local() -> Arc<FsWorkerLocal> {
    Arc::new(FsWorkerLocal {
        base: FakeWorkerLocal::new(),
        session_id: 1,
        fs: Arc::new(MockFsService::new()),
    })
}

/// Register a task context bound to an arbitrary worker-local (generic form
/// of `relation_context`).
pub(crate) fn task_context(oid: OID, wl: WorkerLocalRef) {
    let conn = MuduConnAsync::new_with_worker_local(wl).unwrap();
    Context::create(oid, DBConn::Async(Arc::new(conn))).unwrap();
}
