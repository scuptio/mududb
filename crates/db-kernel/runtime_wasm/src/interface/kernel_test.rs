#![allow(clippy::unimplemented)]

use super::kernel_async::*;
use super::kernel_sync::*;
use ::catalog::contract::message_bus_api::{
    Envelope, MessageBus, MessageId, OnRecvCallback, OutgoingMessage, RecvFilter, SubscriptionId,
};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::worker_snapshot::KvItem;
use ::conn::mudu_conn_async::MuduConnAsync;
use ::conn::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
use ::xengine::DataBin;
use ::xengine::api::{
    AlterTable, DeltaOp, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData,
    VecDatum, VecSelTerm, XContract,
};
use ::xengine::tx_mgr::TxMgr;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::AttrIndex;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_binding::codec::syscall_payload::{
    MessageKind, decode_close_result, decode_delete_result, decode_fs_open_result,
    decode_fs_read_result, decode_fs_readdir_result, decode_get_result, decode_open_result,
    decode_put_result, decode_range_result, decode_relation_get_result,
    decode_relation_insert_result, decode_relation_update_result, encode_close_request,
    encode_delete_request, encode_frame, encode_fs_open_request, encode_fs_read_request,
    encode_fs_readdir_request, encode_get_request, encode_open_request, encode_put_request,
    encode_range_request, encode_relation_get_request, encode_relation_insert_request,
    encode_relation_update_request,
};
use mudu_binding::universal::mp_wire::{FromValue, ToValue, decode_value, encode_value};
use mudu_binding::universal::uni_error::UniError;
use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_result::UniResult;
use mudu_binding::universal::uni_result_set::UniResultSet;
use mudu_contract::database::db_conn::DBConnSync;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::result_set::{ResultSet, ResultSetAsync};
use mudu_contract::database::sql::{Context, DBConn};
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_value::DataValue;
use std::collections::HashMap;
use std::sync::Arc;

use mudu_sys::sync::SMutex;

mod fetch;
mod kv_fs_frames;
mod relation;
mod session;
mod sql_errors;

struct NullXContract;

#[async_trait]
impl XContract for NullXContract {
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
    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
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
    async fn list_schemas(&self) -> RS<Vec<SchemaTable>> {
        unimplemented!()
    }
}

struct NullMessageBus;

#[async_trait]
impl MessageBus for NullMessageBus {
    fn local_endpoint(&self) -> OID {
        unimplemented!()
    }
    async fn send(&self, _dst: OID, _message: OutgoingMessage) -> RS<MessageId> {
        unimplemented!()
    }
    async fn recv(&self, _filter: RecvFilter) -> RS<Envelope> {
        unimplemented!()
    }
    fn on_recv_callback(
        &self,
        _filter: RecvFilter,
        _callback: OnRecvCallback,
    ) -> RS<SubscriptionId> {
        unimplemented!()
    }
    fn cancel_callback(&self, _id: SubscriptionId) -> RS<bool> {
        unimplemented!()
    }
}

struct FakeWorkerLocal {
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
    fn message_bus(&self) -> Arc<dyn MessageBus> {
        Arc::new(NullMessageBus)
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

fn worker_local() -> WorkerLocalRef {
    Arc::new(FakeWorkerLocal::new())
}

/// A worker-local that only serves sessions it minted itself:
/// `open_async` issues and registers real session ids, and every
/// session-bound operation rejects unknown ids — the same contract
/// `WorkerSessionManager` enforces. A syscall that forwards the
/// guest-side task id (instead of resolving the task context's
/// connection session) fails here, reproducing the pre-fix KV behavior.
struct SessionAwareWorkerLocal {
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
                ErrorCode::EntityNotFound,
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
    fn message_bus(&self) -> Arc<dyn MessageBus> {
        self.base.message_bus()
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

fn session_aware_worker_local() -> Arc<SessionAwareWorkerLocal> {
    Arc::new(SessionAwareWorkerLocal::new())
}

/// A worker-local with a minimal in-memory relation store, used to verify
/// the relation syscall plumbing (frame decode, attribute/delta mapping).
type RelationRows = HashMap<(String, Vec<u8>), Vec<(AttrIndex, DataBin)>>;

struct RelationWorkerLocal {
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
    fn message_bus(&self) -> Arc<dyn MessageBus> {
        self.base.message_bus()
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
                ErrorCode::EntityAlreadyExists,
                "existing key"
            ));
        }
        let mut row = key;
        row.extend(values);
        rows.insert(id, row);
        Ok(())
    }
}

fn relation_worker_local() -> WorkerLocalRef {
    Arc::new(RelationWorkerLocal::new())
}

/// Register a task context whose async connection is bound to `wl` — the
/// same binding `app_inst_impl` creates for a procedure invocation.
/// Relation syscalls resolve the connection (and with it the session)
/// through this context; the frame's session field is only the lookup
/// key.
fn relation_context(oid: OID, wl: WorkerLocalRef) {
    let conn = MuduConnAsync::new_with_worker_local(wl).unwrap();
    Context::create(oid, DBConn::Async(Arc::new(conn))).unwrap();
}
