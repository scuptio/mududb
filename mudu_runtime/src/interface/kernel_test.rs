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
    use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
    use mudu_binding::universal::uni_oid::UniOid;
    use mudu_contract::database::result_set::ResultSetAsync;
    use mudu_contract::database::sql::{Context, DBConn};
    use mudu_contract::database::sql_params::SQLParams;
    use mudu_contract::database::sql_stmt::SQLStmt;
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
        async fn get_table_by_name(&self, _name: &str) -> RS<Option<Arc<TableDesc>>> {
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
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<Arc<dyn ResultSetAsync>> {
            unimplemented!()
        }
        async fn execute(
            &self,
            _oid: OID,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            unimplemented!()
        }
        async fn batch(
            &self,
            _oid: OID,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            unimplemented!()
        }
    }

    fn worker_local() -> WorkerLocalRef {
        Arc::new(FakeWorkerLocal::new())
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
            sql: Box<dyn SQLStmt>,
            param: Box<dyn SQLParams>,
        ) -> RS<Arc<dyn ResultSetAsync>> {
            self.base.query(oid, sql, param).await
        }
        async fn execute(
            &self,
            oid: OID,
            sql: Box<dyn SQLStmt>,
            param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            self.base.execute(oid, sql, param).await
        }
        async fn batch(
            &self,
            oid: OID,
            sql: Box<dyn SQLStmt>,
            param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            self.base.batch(oid, sql, param).await
        }
        async fn relation_get(
            &self,
            _session_id: OID,
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
        assert!(fetch_internal(b"ignored").is_empty());
    }

    #[tokio::test]
    async fn async_fetch_internal_returns_empty() {
        assert!(async_fetch_internal(vec![]).await.is_empty());
    }

    #[test]
    fn open_and_close_with_worker_local_round_trip() {
        let wl = worker_local();
        let open_in = encode_open_request(UniOid::from_oid(0));
        let open_out = open_internal_with_worker_local(&open_in, Some(wl.clone()));
        let session_id = decode_open_result(&open_out).unwrap();
        assert_ne!(session_id, 0);

        let close_in = encode_close_request(UniOid::from_oid(session_id));
        let close_out = close_internal_with_worker_local(&close_in, Some(wl.clone()));
        decode_close_result(&close_out).unwrap();
    }

    #[test]
    fn kv_operations_with_worker_local_round_trip() {
        let wl = worker_local();
        let sid = UniOid::from_oid(1);

        let put_in = encode_put_request(sid.clone(), b"alpha", b"1");
        let put_out = put_internal_with_worker_local(&put_in, Some(wl.clone()));
        decode_put_result(&put_out).unwrap();

        let get_in = encode_get_request(sid.clone(), b"alpha");
        let get_out = get_internal_with_worker_local(&get_in, Some(wl.clone()));
        let value = decode_get_result(&get_out).unwrap();
        assert_eq!(value, Some(b"1".to_vec()));

        let range_in = encode_range_request(sid.clone(), b"a", b"z");
        let range_out = range_internal_with_worker_local(&range_in, Some(wl.clone()));
        let items = decode_range_result(&range_out).unwrap();
        assert_eq!(items, vec![(b"alpha".to_vec(), b"1".to_vec())]);

        let delete_in = encode_delete_request(sid, b"alpha");
        let delete_out = delete_internal_with_worker_local(&delete_in, Some(wl.clone()));
        decode_delete_result(&delete_out).unwrap();

        let get_out2 = get_internal_with_worker_local(&get_in, Some(wl.clone()));
        assert_eq!(decode_get_result(&get_out2).unwrap(), None);
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

    fn fs_open_frame() -> Vec<u8> {
        encode_fs_open_request(&UniFsOpenArgv {
            session: UniOid::from_oid(1),
            oid: UniOid::from_oid(2),
            path: "data.bin".to_string(),
            flags: 0,
        })
    }

    #[test]
    fn fs_syscalls_without_worker_local_return_not_implemented() {
        let out = fs_open_internal_with_worker_local(&fs_open_frame(), None);
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

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
        // reports the fs syscalls as unavailable.
        let wl = worker_local();

        let out = fs_open_internal_with_worker_local(&fs_open_frame(), Some(wl.clone()));
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

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
        let sid = UniOid::from_oid(1);

        let put_in = encode_put_request(sid.clone(), b"beta", b"2");
        let put_out = async_put_internal_with_worker_local(put_in, Some(wl.clone())).await;
        decode_put_result(&put_out).unwrap();

        let get_in = encode_get_request(sid.clone(), b"beta");
        let get_out = async_get_internal_with_worker_local(get_in, Some(wl.clone())).await;
        let value = decode_get_result(&get_out).unwrap();
        assert_eq!(value, Some(b"2".to_vec()));

        let delete_in = encode_delete_request(sid.clone(), b"beta");
        let delete_out = async_delete_internal_with_worker_local(delete_in, Some(wl.clone())).await;
        decode_delete_result(&delete_out).unwrap();

        let range_in = encode_range_request(sid, b"a", b"z");
        let range_out = async_range_internal_with_worker_local(range_in, Some(wl.clone())).await;
        let items = decode_range_result(&range_out).unwrap();
        assert!(items.is_empty());

        let open_in = encode_open_request(UniOid::from_oid(0));
        let open_out = async_open_internal_with_worker_local(open_in, Some(wl.clone())).await;
        let session_id = decode_open_result(&open_out).unwrap();

        let close_in = encode_close_request(UniOid::from_oid(session_id));
        let close_out = async_close_internal_with_worker_local(close_in, Some(wl.clone())).await;
        decode_close_result(&close_out).unwrap();
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
        // No worker local configured: NotImplemented in the MSSP error envelope.
        let out = async_fs_open_internal_with_worker_local(fs_open_frame(), None).await;
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

        // Worker local without an fs service (trait default): NotImplemented.
        let out =
            async_fs_open_internal_with_worker_local(fs_open_frame(), Some(worker_local())).await;
        let err = decode_fs_open_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

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
