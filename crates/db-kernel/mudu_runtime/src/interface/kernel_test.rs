#[cfg(test)]
mod tests {
    #![allow(clippy::unimplemented)]

    use super::super::kernel_async::*;
    use super::super::kernel_sync::*;
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
    use mudu_kernel::contract::meta_mgr::MetaMgr;
    use mudu_kernel::contract::partition_rule::PartitionRuleDesc;
    use mudu_kernel::contract::partition_rule_binding::{
        PartitionPlacement, TablePartitionBinding,
    };
    use mudu_kernel::contract::schema_table::SchemaTable;
    use mudu_kernel::contract::table_desc::TableDesc;
    use mudu_kernel::mudu_conn::mudu_conn_async::MuduConnAsync;
    use mudu_kernel::server::message_bus_api::{
        Envelope, MessageBus, MessageId, OnRecvCallback, OutgoingMessage, RecvFilter,
        SubscriptionId,
    };
    use mudu_kernel::server::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
    use mudu_kernel::server::worker_snapshot::KvItem;
    use mudu_kernel::x_engine::DataBin;
    use mudu_kernel::x_engine::api::{
        AlterTable, DeltaOp, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor,
        RangeData, VecDatum, VecSelTerm, XContract,
    };
    use mudu_kernel::x_engine::tx_mgr::TxMgr;
    use mudu_type::data_value::DataValue;
    use std::collections::HashMap;
    use std::sync::Arc;

    use mudu_sys::sync::SMutex;

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
        async fn get_table_by_name(
            &self,
            _schema: &str,
            _name: &str,
        ) -> RS<Option<Arc<TableDesc>>> {
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

    #[test]
    fn sync_kv_syscalls_resolve_session_through_task_context() {
        let fake = session_aware_worker_local();
        let wl: WorkerLocalRef = fake;
        // The frame's session field is a task id that is deliberately not a
        // worker session id: only resolving the task context's connection
        // (and its lazily opened worker session) can complete the operation.
        let task = UniOid::from_oid(8_200_001);
        relation_context(task.to_oid(), wl);

        let put_in = encode_put_request(task.clone(), b"alpha", b"1");
        decode_put_result(&put_internal_with_worker_local(&put_in, None)).unwrap();

        let get_in = encode_get_request(task.clone(), b"alpha");
        let get_out = get_internal_with_worker_local(&get_in, None);
        assert_eq!(decode_get_result(&get_out).unwrap(), Some(b"1".to_vec()));

        let range_in = encode_range_request(task.clone(), b"a", b"z");
        let range_out = range_internal_with_worker_local(&range_in, None);
        assert_eq!(
            decode_range_result(&range_out).unwrap(),
            vec![(b"alpha".to_vec(), b"1".to_vec())]
        );

        let delete_in = encode_delete_request(task.clone(), b"alpha");
        decode_delete_result(&delete_internal_with_worker_local(&delete_in, None)).unwrap();
        let get_out2 = get_internal_with_worker_local(&get_in, None);
        assert_eq!(decode_get_result(&get_out2).unwrap(), None);

        Context::remove(task.to_oid());
    }

    #[tokio::test]
    async fn async_kv_syscalls_resolve_session_through_task_context() {
        let fake = session_aware_worker_local();
        let wl: WorkerLocalRef = fake;
        let task = UniOid::from_oid(8_200_002);
        relation_context(task.to_oid(), wl);

        let put_in = encode_put_request(task.clone(), b"beta", b"2");
        decode_put_result(&async_put_internal_with_worker_local(put_in, None).await).unwrap();

        let get_in = encode_get_request(task.clone(), b"beta");
        let get_out = async_get_internal_with_worker_local(get_in, None).await;
        assert_eq!(decode_get_result(&get_out).unwrap(), Some(b"2".to_vec()));

        let range_in = encode_range_request(task.clone(), b"a", b"z");
        let range_out = async_range_internal_with_worker_local(range_in, None).await;
        assert_eq!(
            decode_range_result(&range_out).unwrap(),
            vec![(b"beta".to_vec(), b"2".to_vec())]
        );

        let delete_in = encode_delete_request(task.clone(), b"beta");
        decode_delete_result(&async_delete_internal_with_worker_local(delete_in, None).await)
            .unwrap();
        let get_out2 =
            async_get_internal_with_worker_local(encode_get_request(task.clone(), b"beta"), None)
                .await;
        assert_eq!(decode_get_result(&get_out2).unwrap(), None);

        Context::remove(task.to_oid());
    }

    #[test]
    fn sync_close_closes_session_through_task_context() {
        let fake = session_aware_worker_local();
        let wl: WorkerLocalRef = fake.clone();
        let task = UniOid::from_oid(8_200_003);
        relation_context(task.to_oid(), wl);

        // Closing a connection that never opened a session is a no-op.
        let close_in = encode_close_request(task.clone());
        decode_close_result(&close_internal_with_worker_local(&close_in, None)).unwrap();

        // A KV operation opens the connection's worker session; close then
        // retires exactly that session.
        let put_in = encode_put_request(task.clone(), b"k", b"v");
        decode_put_result(&put_internal_with_worker_local(&put_in, None)).unwrap();
        assert_eq!(fake.live_session_count(), 1);
        decode_close_result(&close_internal_with_worker_local(&close_in, None)).unwrap();
        assert_eq!(fake.live_session_count(), 0);

        Context::remove(task.to_oid());
    }

    #[tokio::test]
    async fn async_close_closes_session_through_task_context() {
        let fake = session_aware_worker_local();
        let wl: WorkerLocalRef = fake.clone();
        let task = UniOid::from_oid(8_200_004);
        relation_context(task.to_oid(), wl);

        let close_in = encode_close_request(task.clone());
        decode_close_result(&async_close_internal_with_worker_local(close_in.clone(), None).await)
            .unwrap();

        let put_in = encode_put_request(task.clone(), b"k", b"v");
        decode_put_result(&async_put_internal_with_worker_local(put_in, None).await).unwrap();
        assert_eq!(fake.live_session_count(), 1);
        decode_close_result(&async_close_internal_with_worker_local(close_in, None).await).unwrap();
        assert_eq!(fake.live_session_count(), 0);

        Context::remove(task.to_oid());
    }

    #[test]
    fn sync_close_closes_open_minted_session_directly() {
        // The `open` syscall mints worker session ids that are never in the
        // task Context registry; closing such an id goes straight to the
        // worker local (the py-spike open/close roundtrip).
        let fake = session_aware_worker_local();
        let wl: WorkerLocalRef = fake.clone();
        let session = {
            let fake = fake.clone();
            crate::async_utils::blocking::run_async(async move { fake.open_async().await })
                .unwrap()
                .unwrap()
        };
        assert_eq!(fake.live_session_count(), 1);
        let close_in = encode_close_request(UniOid::from_oid(session));
        decode_close_result(&close_internal_with_worker_local(&close_in, Some(wl))).unwrap();
        assert_eq!(fake.live_session_count(), 0);
    }

    #[tokio::test]
    async fn async_close_closes_open_minted_session_directly() {
        let fake = session_aware_worker_local();
        let wl: WorkerLocalRef = fake.clone();
        let session = fake.open_async().await.unwrap();
        assert_eq!(fake.live_session_count(), 1);
        let close_in = encode_close_request(UniOid::from_oid(session));
        decode_close_result(&async_close_internal_with_worker_local(close_in, Some(wl)).await)
            .unwrap();
        assert_eq!(fake.live_session_count(), 0);
    }

    #[tokio::test]
    async fn async_relation_syscalls_round_trip() {
        let sid = UniOid::from_oid(7);
        relation_context(sid.to_oid(), relation_worker_local());

        let insert_in = encode_relation_insert_request(
            sid.clone(),
            "district",
            &[(1, &b"w"[..]), (0, &b"d"[..])],
            &[(5, &b"n"[..])],
        );
        let insert_out = async_relation_insert_internal_with_worker_local(insert_in, None).await;
        decode_relation_insert_result(&insert_out).unwrap();

        // Duplicate primary key: EntityAlreadyExists in the error envelope.
        let dup_in = encode_relation_insert_request(
            sid.clone(),
            "district",
            &[(1, &b"w"[..]), (0, &b"d"[..])],
            &[],
        );
        let dup_out = async_relation_insert_internal_with_worker_local(dup_in, None).await;
        let err = decode_relation_insert_result(&dup_out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityAlreadyExists);

        let get_in = encode_relation_get_request(
            sid.clone(),
            "district",
            &[(1, &b"w"[..]), (0, &b"d"[..])],
            &[5, 3],
        );
        let get_out = async_relation_get_internal_with_worker_local(get_in, None).await;
        let row = decode_relation_get_result(&get_out).unwrap();
        assert_eq!(row, Some(vec![Some(b"n".to_vec()), None]));

        let update_in = encode_relation_update_request(
            sid.clone(),
            "district",
            &[(1, &b"w"[..]), (0, &b"d"[..])],
            &[(5, &b"m"[..])],
            &[(5, 0, &b"1"[..]), (4, 1, &b"2"[..])],
        );
        let update_out = async_relation_update_internal_with_worker_local(update_in, None).await;
        assert_eq!(decode_relation_update_result(&update_out).unwrap(), 1);

        let get_in2 = encode_relation_get_request(
            sid.clone(),
            "district",
            &[(1, &b"w"[..]), (0, &b"d"[..])],
            &[5],
        );
        let get_out2 = async_relation_get_internal_with_worker_local(get_in2, None).await;
        assert_eq!(
            decode_relation_get_result(&get_out2).unwrap(),
            Some(vec![Some(b"m".to_vec())])
        );

        // Unknown delta op code: Decode error in the error envelope.
        let bad_op_in = encode_relation_update_request(
            sid.clone(),
            "district",
            &[(1, &b"w"[..]), (0, &b"d"[..])],
            &[],
            &[(5, 9, &b"1"[..])],
        );
        let bad_op_out = async_relation_update_internal_with_worker_local(bad_op_in, None).await;
        let err = decode_relation_update_result(&bad_op_out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);

        // Missing key: get returns None, update affects zero rows.
        let miss_get_in =
            encode_relation_get_request(sid.clone(), "district", &[(0, &b"x"[..])], &[5]);
        let miss_get_out = async_relation_get_internal_with_worker_local(miss_get_in, None).await;
        assert_eq!(decode_relation_get_result(&miss_get_out).unwrap(), None);

        Context::remove(sid.to_oid());
    }

    #[test]
    fn sync_relation_syscalls_round_trip() {
        let sid = UniOid::from_oid(8);
        relation_context(sid.to_oid(), relation_worker_local());

        let insert_in = encode_relation_insert_request(
            sid.clone(),
            "orders",
            &[(0, &b"o"[..])],
            &[(3, &b"c"[..])],
        );
        let insert_out = relation_insert_internal_with_worker_local(&insert_in, None);
        decode_relation_insert_result(&insert_out).unwrap();

        let get_in = encode_relation_get_request(sid.clone(), "orders", &[(0, &b"o"[..])], &[3]);
        let get_out = relation_get_internal_with_worker_local(&get_in, None);
        assert_eq!(
            decode_relation_get_result(&get_out).unwrap(),
            Some(vec![Some(b"c".to_vec())])
        );

        let update_in = encode_relation_update_request(
            sid.clone(),
            "orders",
            &[(0, &b"missing"[..])],
            &[(3, &b"x"[..])],
            &[],
        );
        let update_out = relation_update_internal_with_worker_local(&update_in, None);
        assert_eq!(decode_relation_update_result(&update_out).unwrap(), 0);

        Context::remove(sid.to_oid());
    }

    #[test]
    fn relation_syscalls_require_task_context() {
        // Without a registered task context the frame's session id resolves
        // to nothing and the syscall reports EntityNotFound.
        let missing = UniOid::from_oid(987_654_321);
        let get_in = encode_relation_get_request(missing.clone(), "t", &[(0, &b"k"[..])], &[0]);
        let out = relation_get_internal_with_worker_local(&get_in, None);
        let err = decode_relation_get_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        let update_in =
            encode_relation_update_request(missing.clone(), "t", &[(0, &b"k"[..])], &[], &[]);
        let out = relation_update_internal_with_worker_local(&update_in, None);
        let err = decode_relation_update_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        let insert_in = encode_relation_insert_request(missing, "t", &[(0, &b"k"[..])], &[]);
        let out = relation_insert_internal_with_worker_local(&insert_in, None);
        let err = decode_relation_insert_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    }

    #[tokio::test]
    async fn async_relation_syscalls_require_task_context_and_relation_support() {
        let missing = UniOid::from_oid(987_654_322);
        let get_in = encode_relation_get_request(missing, "t", &[(0, &b"k"[..])], &[0]);
        let out = async_relation_get_internal_with_worker_local(get_in, None).await;
        let err = decode_relation_get_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // The trait default (FakeWorkerLocal does not override relation_*)
        // reports NotImplemented once the task context resolves.
        let sid = UniOid::from_oid(987_654_323);
        relation_context(sid.to_oid(), worker_local());
        let get_in = encode_relation_get_request(sid.clone(), "t", &[(0, &b"k"[..])], &[0]);
        let out = async_relation_get_internal_with_worker_local(get_in, None).await;
        let err = decode_relation_get_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

        Context::remove(sid.to_oid());
    }

    #[test]
    fn query_internal_reports_decode_error_for_invalid_bytes() {
        let bytes = b"not a valid query payload";
        let out = query_internal(bytes);
        assert!(!out.is_empty());
        let result = mudu_binding::system::query_invoke::deserialize_query_result(&out);
        match result {
            Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn query_internal_reports_missing_context_error() {
        let bytes = mudu_binding::system::query_invoke::serialize_query_dyn_param(
            999u128,
            &"SELECT 1",
            &(),
        )
        .unwrap();
        let out = query_internal(&bytes);
        assert!(!out.is_empty());
        let result = mudu_binding::system::query_invoke::deserialize_query_result(&out);
        match result {
            Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn command_internal_reports_missing_context_error() {
        let bytes = mudu_binding::system::command_invoke::serialize_command_param(
            999u128,
            &"INSERT INTO t VALUES (1)",
            &(),
        )
        .unwrap();
        let out = command_internal(&bytes);
        assert!(!out.is_empty());
        let err =
            mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
    }

    #[test]
    fn batch_internal_reports_missing_context_error() {
        let bytes = mudu_binding::system::command_invoke::serialize_command_param(
            999u128,
            &"INSERT INTO t VALUES (1)",
            &(),
        )
        .unwrap();
        let out = batch_internal(&bytes);
        assert!(!out.is_empty());
        let err =
            mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
    }

    #[test]
    fn empty_sql_syscalls_return_empty() {
        assert!(empty_query_internal(b"ignored").is_empty());
        assert!(empty_command_internal(b"ignored").is_empty());
    }

    /// Minimal sync connection returning a fixed row set, mirroring the
    /// sys_interface fetch tests (`MockDBConnSync` in
    /// `crates/common/sys_interface/src/api_impl/sync.rs`).
    struct MockResultSet {
        rows: SMutex<Vec<Option<TupleValue>>>,
    }

    impl MockResultSet {
        fn new(rows: Vec<TupleValue>) -> Self {
            Self {
                rows: SMutex::new(rows.into_iter().map(Some).collect()),
            }
        }
    }

    impl ResultSet for MockResultSet {
        fn next(&self) -> RS<Option<TupleValue>> {
            let mut rows = self.rows.lock().unwrap();
            Ok(if rows.is_empty() {
                None
            } else {
                rows.remove(0)
            })
        }
    }

    struct MockDBConnSync {
        query_result: RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>,
    }

    impl MockDBConnSync {
        fn with_query(rows: Vec<TupleValue>) -> Self {
            let desc = i32::tuple_desc().clone();
            Self {
                query_result: Ok((Arc::new(MockResultSet::new(rows)), Arc::new(desc))),
            }
        }
    }

    impl DBConnSync for MockDBConnSync {
        fn exec_silent(&self, _sql_text: &str) -> RS<()> {
            Ok(())
        }

        fn begin_tx(&self) -> RS<OID> {
            Ok(1)
        }

        fn rollback_tx(&self) -> RS<()> {
            Ok(())
        }

        fn commit_tx(&self) -> RS<()> {
            Ok(())
        }

        fn query(
            &self,
            _sql: &dyn SQLStmt,
            _param: &dyn SQLParams,
        ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
            self.query_result.clone()
        }

        fn command(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            Ok(0)
        }

        fn batch(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            Ok(0)
        }
    }

    fn query_context(oid: OID, rows: Vec<TupleValue>) -> Context {
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_query(rows)));
        Context::create(oid, conn).unwrap()
    }

    fn fetch_cursor(oid: OID) -> Vec<u8> {
        encode_value(&UniOid::from(oid).to_value())
    }

    fn decode_fetch_result(bytes: &[u8]) -> UniResult<UniResultSet, UniError> {
        UniResult::from_value(&decode_value(bytes).unwrap()).unwrap()
    }

    fn first_i32_from_uni_result_set(rs: &UniResultSet) -> i32 {
        *rs.row_set[0].fields[0]
            .as_scalar()
            .unwrap()
            .as_i32()
            .unwrap()
    }

    #[test]
    fn fetch_internal_drains_cached_rows() {
        // Mirrors sys_interface's `mudu_fetch_bytes_drains_cached_rows`:
        // rows cached on the context are drained by the first fetch; the
        // second fetch finds the cache cleared and reports eof with no rows.
        let oid = 9_100_001;
        let rows = vec![
            TupleValue::from(vec![DataValue::from_i32(10)]),
            TupleValue::from(vec![DataValue::from_i32(20)]),
        ];
        let ctx = query_context(oid, rows);
        let (rs, desc) = ctx.query_raw(&"SELECT 1", &()).unwrap();
        ctx.cache_result((rs, desc)).unwrap();

        let out = fetch_internal(&fetch_cursor(oid));
        let result_set = match decode_fetch_result(&out) {
            UniResult::Ok(rs) => rs,
            UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
        };
        assert!(result_set.eof);
        assert_eq!(result_set.row_set.len(), 2);
        assert_eq!(first_i32_from_uni_result_set(&result_set), 10);
        // The response cursor round-trips the context oid.
        let cursor_oid = UniOid::from_value(&decode_value(&result_set.cursor).unwrap()).unwrap();
        assert_eq!(cursor_oid.to_oid(), oid);

        let out = fetch_internal(&fetch_cursor(oid));
        match decode_fetch_result(&out) {
            UniResult::Ok(rs) => {
                assert!(rs.eof);
                assert!(rs.row_set.is_empty());
            }
            UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
        }

        Context::remove(oid);
    }

    #[test]
    fn fetch_internal_after_query_reports_eof_with_no_rows() {
        // The query path drains the cached result set into the query
        // response itself, so a subsequent fetch returns an empty, eof
        // batch (sys_interface semantics).
        let oid = 9_100_002;
        let rows = vec![TupleValue::from(vec![DataValue::from_i32(42)])];
        let _ctx = query_context(oid, rows);

        let query_in =
            mudu_binding::system::query_invoke::serialize_query_dyn_param(oid, &"SELECT 1", &())
                .unwrap();
        let out = query_internal(&query_in);
        let (batch, _desc) =
            mudu_binding::system::query_invoke::deserialize_query_result(&out).unwrap();
        assert_eq!(batch.rows().len(), 1);
        assert!(batch.is_eof());

        let out = fetch_internal(&fetch_cursor(oid));
        match decode_fetch_result(&out) {
            UniResult::Ok(rs) => {
                assert!(rs.eof);
                assert!(rs.row_set.is_empty());
            }
            UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
        }

        Context::remove(oid);
    }

    #[test]
    fn fetch_internal_unknown_cursor_returns_uni_error() {
        let out = fetch_internal(&fetch_cursor(9_100_003));
        match decode_fetch_result(&out) {
            UniResult::Err(err) => {
                assert_eq!(err.err_code, ErrorCode::EntityNotFound.to_u32())
            }
            UniResult::Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn fetch_internal_malformed_cursor_returns_uni_error() {
        // 0xc1 is a reserved MessagePack marker: decoding it as UniOid fails.
        let out = fetch_internal(&[0xc1]);
        match decode_fetch_result(&out) {
            UniResult::Err(_) => {}
            UniResult::Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn async_fetch_internal_drains_cached_rows() {
        let oid = 9_100_004;
        let rows = vec![TupleValue::from(vec![DataValue::from_i32(7)])];
        let ctx = query_context(oid, rows);
        let (rs, desc) = ctx.query_raw(&"SELECT 1", &()).unwrap();
        ctx.cache_result((rs, desc)).unwrap();

        let out = async_fetch_internal(fetch_cursor(oid)).await;
        match decode_fetch_result(&out) {
            UniResult::Ok(rs) => {
                assert!(rs.eof);
                assert_eq!(rs.row_set.len(), 1);
                assert_eq!(first_i32_from_uni_result_set(&rs), 7);
            }
            UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
        }

        Context::remove(oid);
    }

    #[tokio::test]
    async fn async_fetch_internal_unknown_cursor_returns_uni_error() {
        let out = async_fetch_internal(fetch_cursor(9_100_005)).await;
        match decode_fetch_result(&out) {
            UniResult::Err(err) => {
                assert_eq!(err.err_code, ErrorCode::EntityNotFound.to_u32())
            }
            UniResult::Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn open_with_worker_local_mints_session() {
        let wl = worker_local();
        let open_in = encode_open_request(UniOid::from_oid(0));
        let open_out = open_internal_with_worker_local(&open_in, Some(wl.clone()));
        let session_id = decode_open_result(&open_out).unwrap();
        assert_ne!(session_id, 0);
    }

    #[test]
    fn kv_operations_with_worker_local_round_trip() {
        let wl = worker_local();
        let sid = UniOid::from_oid(8_200_010);
        relation_context(sid.to_oid(), wl);

        let put_in = encode_put_request(sid.clone(), b"alpha", b"1");
        let put_out = put_internal_with_worker_local(&put_in, None);
        decode_put_result(&put_out).unwrap();

        let get_in = encode_get_request(sid.clone(), b"alpha");
        let get_out = get_internal_with_worker_local(&get_in, None);
        let value = decode_get_result(&get_out).unwrap();
        assert_eq!(value, Some(b"1".to_vec()));

        let range_in = encode_range_request(sid.clone(), b"a", b"z");
        let range_out = range_internal_with_worker_local(&range_in, None);
        let items = decode_range_result(&range_out).unwrap();
        assert_eq!(items, vec![(b"alpha".to_vec(), b"1".to_vec())]);

        let delete_in = encode_delete_request(sid.clone(), b"alpha");
        let delete_out = delete_internal_with_worker_local(&delete_in, None);
        decode_delete_result(&delete_out).unwrap();

        let get_out2 = get_internal_with_worker_local(&get_in, None);
        assert_eq!(decode_get_result(&get_out2).unwrap(), None);

        Context::remove(sid.to_oid());
    }

    #[test]
    fn kv_operations_without_worker_local_return_error_frames() {
        let get_in = encode_get_request(UniOid::from_oid(1), b"alpha");
        let out = get_internal(&get_in);
        assert!(decode_get_result(&out).is_err());

        let put_in = encode_put_request(UniOid::from_oid(1), b"alpha", b"1");
        let out = put_internal(&put_in);
        assert!(decode_put_result(&out).is_err());

        let delete_in = encode_delete_request(UniOid::from_oid(1), b"alpha");
        let out = delete_internal(&delete_in);
        assert!(decode_delete_result(&out).is_err());

        let range_in = encode_range_request(UniOid::from_oid(1), b"a", b"z");
        let out = range_internal(&range_in);
        assert!(decode_range_result(&out).is_err());
    }

    fn fs_open_frame_for(session: UniOid) -> Vec<u8> {
        encode_fs_open_request(&UniFsOpenArgv {
            session,
            oid: UniOid::from_oid(2),
            path: "data.bin".to_string(),
            flags: 0,
        })
    }

    #[test]
    fn fs_syscalls_without_task_context_return_error_frames() {
        // fs-open resolves the task context's connection: with no context
        // registered the frame reports EntityNotFound.
        let out = fs_open_internal_with_worker_local(
            &fs_open_frame_for(UniOid::from_oid(8_200_020)),
            None,
        );
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // The fd/path-based fs syscalls still resolve through the passed
        // worker-local interface, which is absent here.
        let read_in = encode_fs_read_request(3, 16);
        let out = fs_read_internal_with_worker_local(&read_in, None);
        let err = decode_fs_read_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

        let readdir_in = encode_fs_readdir_request(UniOid::from_oid(2), "docs");
        let out = fs_readdir_internal_with_worker_local(&readdir_in, None);
        let err = decode_fs_readdir_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[test]
    fn fs_syscalls_with_default_fs_service_return_not_implemented() {
        // FakeWorkerLocal does not override fs_service(); the trait default
        // reports the fs syscalls as unavailable. fs-open reaches that
        // default through the task context's connection.
        let wl = worker_local();
        let sid = UniOid::from_oid(8_200_021);
        relation_context(sid.to_oid(), wl.clone());
        let out = fs_open_internal_with_worker_local(&fs_open_frame_for(sid.clone()), None);
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        assert!(err.message().contains("not available on this worker"));
        Context::remove(sid.to_oid());

        let read_in = encode_fs_read_request(3, 16);
        let out = fs_read_internal_with_worker_local(&read_in, Some(wl.clone()));
        let err = decode_fs_read_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

        let readdir_in = encode_fs_readdir_request(UniOid::from_oid(2), "docs");
        let out = fs_readdir_internal_with_worker_local(&readdir_in, Some(wl));
        let err = decode_fs_readdir_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[test]
    fn fs_syscalls_malformed_frames_return_error_frames() {
        // Fewer bytes than the MSSP header: corrupted data.
        let out = fs_open_internal_with_worker_local(&[0x01, 0x02], None);
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::CorruptedData);

        // A valid header with a malformed MessagePack body: decode error.
        let bad_body = encode_frame(MessageKind::FsOpen, &[0xff]);
        let out = fs_open_internal_with_worker_local(&bad_body, None);
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);

        // A well-formed frame of a different message kind: decode error.
        let wrong_kind = encode_fs_read_request(3, 16);
        let out = fs_open_internal_with_worker_local(&wrong_kind, None);
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);

        // The same header rules apply to the other fs handlers.
        let out = fs_read_internal_with_worker_local(&[0xff], None);
        let err = decode_fs_read_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::CorruptedData);

        let out = fs_readdir_internal_with_worker_local(&[0xde, 0xad, 0xbe], None);
        let err = decode_fs_readdir_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::CorruptedData);
    }

    #[tokio::test]
    async fn async_kv_operations_with_worker_local_round_trip() {
        let wl = worker_local();
        let sid = UniOid::from_oid(8_200_011);
        relation_context(sid.to_oid(), wl.clone());

        let put_in = encode_put_request(sid.clone(), b"beta", b"2");
        let put_out = async_put_internal_with_worker_local(put_in, None).await;
        decode_put_result(&put_out).unwrap();

        let get_in = encode_get_request(sid.clone(), b"beta");
        let get_out = async_get_internal_with_worker_local(get_in, None).await;
        let value = decode_get_result(&get_out).unwrap();
        assert_eq!(value, Some(b"2".to_vec()));

        let delete_in = encode_delete_request(sid.clone(), b"beta");
        let delete_out = async_delete_internal_with_worker_local(delete_in, None).await;
        decode_delete_result(&delete_out).unwrap();

        let range_in = encode_range_request(sid.clone(), b"a", b"z");
        let range_out = async_range_internal_with_worker_local(range_in, None).await;
        let items = decode_range_result(&range_out).unwrap();
        assert!(items.is_empty());

        // open still mints worker sessions directly through the worker-local.
        let open_in = encode_open_request(UniOid::from_oid(0));
        let open_out = async_open_internal_with_worker_local(open_in, Some(wl.clone())).await;
        let session_id = decode_open_result(&open_out).unwrap();
        assert_ne!(session_id, 0);

        // close resolves the task context and retires the connection's
        // session (opened lazily by the KV operations above).
        let close_in = encode_close_request(sid.clone());
        let close_out = async_close_internal_with_worker_local(close_in, None).await;
        decode_close_result(&close_out).unwrap();

        Context::remove(sid.to_oid());
    }

    #[tokio::test]
    async fn async_kv_operations_without_worker_local_return_error_frames() {
        let get_in = encode_get_request(UniOid::from_oid(1), b"alpha");
        let out = async_get_internal(get_in).await;
        assert!(decode_get_result(&out).is_err());

        let put_in = encode_put_request(UniOid::from_oid(1), b"alpha", b"1");
        let out = async_put_internal(put_in).await;
        assert!(decode_put_result(&out).is_err());

        let delete_in = encode_delete_request(UniOid::from_oid(1), b"alpha");
        let out = async_delete_internal(delete_in).await;
        assert!(decode_delete_result(&out).is_err());

        let range_in = encode_range_request(UniOid::from_oid(1), b"a", b"z");
        let out = async_range_internal(range_in).await;
        assert!(decode_range_result(&out).is_err());
    }

    #[tokio::test]
    async fn async_fs_open_error_paths_return_error_frames() {
        // No task context registered: EntityNotFound in the MSSP error envelope.
        let out = async_fs_open_internal_with_worker_local(
            fs_open_frame_for(UniOid::from_oid(8_200_022)),
            None,
        )
        .await;
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // Task context bound to a worker without an fs service (trait
        // default): NotImplemented.
        let sid = UniOid::from_oid(8_200_023);
        relation_context(sid.to_oid(), worker_local());
        let out =
            async_fs_open_internal_with_worker_local(fs_open_frame_for(sid.clone()), None).await;
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        Context::remove(sid.to_oid());

        // Truncated frame: CorruptedData in the MSSP error envelope.
        let out = async_fs_open_internal_with_worker_local(vec![0x01, 0x02], None).await;
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::CorruptedData);
    }

    #[tokio::test]
    async fn async_query_reports_missing_context_error() {
        let bytes = mudu_binding::system::query_invoke::serialize_query_dyn_param(
            999u128,
            &"SELECT 1",
            &(),
        )
        .unwrap();
        let out = async_query_internal(bytes).await;
        let result = mudu_binding::system::query_invoke::deserialize_query_result(&out);
        match result {
            Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn async_command_reports_missing_context_error() {
        let bytes = mudu_binding::system::command_invoke::serialize_command_param(
            999u128,
            &"INSERT INTO t VALUES (1)",
            &(),
        )
        .unwrap();
        let out = async_command_internal(bytes).await;
        let err =
            mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
    }

    #[tokio::test]
    async fn async_batch_reports_missing_context_error() {
        let bytes = mudu_binding::system::command_invoke::serialize_command_param(
            999u128,
            &"INSERT INTO t VALUES (1)",
            &(),
        )
        .unwrap();
        let out = async_batch_internal(bytes).await;
        let err =
            mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
    }
}
