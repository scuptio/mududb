#[cfg(test)]
mod tests {
    #![allow(clippy::unimplemented)]

    use super::super::kernel::*;
    use async_trait::async_trait;
    use mudu::common::buf::Buf;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu_binding::codec::handle_sys_session;
    use mudu_contract::database::result_set::ResultSetAsync;
    use mudu_contract::database::sql_params::SQLParams;
    use mudu_contract::database::sql_stmt::SQLStmt;
    use mudu_kernel::contract::meta_mgr::MetaMgr;
    use mudu_kernel::contract::partition_rule::PartitionRuleDesc;
    use mudu_kernel::contract::partition_rule_binding::{
        PartitionPlacement, TablePartitionBinding,
    };
    use mudu_kernel::contract::schema_table::SchemaTable;
    use mudu_kernel::contract::table_desc::TableDesc;
    use mudu_kernel::server::message_bus_api::{
        Envelope, MessageBus, MessageId, OnRecvCallback, OutgoingMessage, RecvFilter,
        SubscriptionId,
    };
    use mudu_kernel::server::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
    use mudu_kernel::server::worker_snapshot::KvItem;
    use mudu_kernel::x_engine::api::{
        AlterTable, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData,
        VecDatum, VecSelTerm, XContract,
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

    #[test]
    fn query_internal_reports_decode_error_for_invalid_bytes() {
        let bytes = b"not a valid query payload";
        let out = query_internal(bytes);
        assert!(!out.is_empty());
        let result = mudu_binding::system::query_invoke::deserialize_query_result(&out);
        match result {
            Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::Decode),
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
        let open_in = handle_sys_session::serialize_open_param();
        let open_out = open_internal_with_worker_local(&open_in, Some(wl.clone())).unwrap();
        let session_id = handle_sys_session::deserialize_open_result(&open_out).unwrap();
        assert_ne!(session_id, 0);

        let close_in = handle_sys_session::serialize_close_param(session_id);
        let close_out = close_internal_with_worker_local(&close_in, Some(wl.clone())).unwrap();
        handle_sys_session::deserialize_close_result(&close_out).unwrap();
    }

    #[test]
    fn kv_operations_with_worker_local_round_trip() {
        let wl = worker_local();
        let sid = 1u128;

        let put_in = handle_sys_session::serialize_session_put_param(sid, b"alpha", b"1");
        let put_out = put_internal_with_worker_local(&put_in, Some(wl.clone())).unwrap();
        handle_sys_session::deserialize_put_result(&put_out).unwrap();

        let get_in = handle_sys_session::serialize_session_get_param(sid, b"alpha");
        let get_out = get_internal_with_worker_local(&get_in, Some(wl.clone())).unwrap();
        let value = handle_sys_session::deserialize_get_result(&get_out).unwrap();
        assert_eq!(value, Some(b"1".to_vec()));

        let range_in = handle_sys_session::serialize_session_range_param(sid, b"a", b"z");
        let range_out = range_internal_with_worker_local(&range_in, Some(wl.clone())).unwrap();
        let items = handle_sys_session::deserialize_range_result(&range_out).unwrap();
        assert_eq!(items, vec![(b"alpha".to_vec(), b"1".to_vec())]);

        let delete_in = handle_sys_session::serialize_session_delete_param(sid, b"alpha");
        let delete_out = delete_internal_with_worker_local(&delete_in, Some(wl.clone())).unwrap();
        handle_sys_session::deserialize_delete_result(&delete_out).unwrap();

        let get_out2 = get_internal_with_worker_local(&get_in, Some(wl.clone())).unwrap();
        assert_eq!(
            handle_sys_session::deserialize_get_result(&get_out2).unwrap(),
            None
        );
    }

    #[test]
    fn kv_operations_without_worker_local_return_error_bytes() {
        let get_in = handle_sys_session::serialize_session_get_param(1, b"alpha");
        let out = get_internal(&get_in);
        assert!(mudu_binding::codec::handle_sys_session::deserialize_get_result(&out).is_err());

        let put_in = handle_sys_session::serialize_session_put_param(1, b"alpha", b"1");
        let out = put_internal(&put_in);
        assert!(handle_sys_session::deserialize_put_result(&out).is_err());

        let delete_in = handle_sys_session::serialize_session_delete_param(1, b"alpha");
        let out = delete_internal(&delete_in);
        assert!(handle_sys_session::deserialize_delete_result(&out).is_err());

        let range_in = handle_sys_session::serialize_session_range_param(1, b"a", b"z");
        let out = range_internal(&range_in);
        assert!(handle_sys_session::deserialize_range_result(&out).is_err());
    }

    #[tokio::test]
    async fn async_kv_operations_with_worker_local_round_trip() {
        let wl = worker_local();
        let sid = 1u128;

        let put_in = handle_sys_session::serialize_session_put_param(sid, b"beta", b"2");
        let put_out = async_put_internal_with_worker_local(put_in, Some(wl.clone())).await;
        handle_sys_session::deserialize_put_result(&put_out).unwrap();

        let get_in = handle_sys_session::serialize_session_get_param(sid, b"beta");
        let get_out = async_get_internal_with_worker_local(get_in, Some(wl.clone())).await;
        let value = handle_sys_session::deserialize_get_result(&get_out).unwrap();
        assert_eq!(value, Some(b"2".to_vec()));

        let delete_in = handle_sys_session::serialize_session_delete_param(sid, b"beta");
        let delete_out = async_delete_internal_with_worker_local(delete_in, Some(wl.clone())).await;
        handle_sys_session::deserialize_delete_result(&delete_out).unwrap();

        let range_in = handle_sys_session::serialize_session_range_param(sid, b"a", b"z");
        let range_out = async_range_internal_with_worker_local(range_in, Some(wl.clone())).await;
        let items = handle_sys_session::deserialize_range_result(&range_out).unwrap();
        assert!(items.is_empty());

        let open_in = handle_sys_session::serialize_open_param();
        let open_out = async_open_internal_with_worker_local(open_in, Some(wl.clone())).await;
        let session_id = handle_sys_session::deserialize_open_result(&open_out).unwrap();

        let close_in = handle_sys_session::serialize_close_param(session_id);
        let close_out = async_close_internal_with_worker_local(close_in, Some(wl.clone())).await;
        handle_sys_session::deserialize_close_result(&close_out).unwrap();
    }

    #[tokio::test]
    async fn async_kv_operations_without_worker_local_return_error_bytes() {
        let get_in = handle_sys_session::serialize_session_get_param(1, b"alpha");
        let out = async_get_internal(get_in).await;
        assert!(handle_sys_session::deserialize_get_result(&out).is_err());

        let put_in = handle_sys_session::serialize_session_put_param(1, b"alpha", b"1");
        let out = async_put_internal(put_in).await;
        assert!(handle_sys_session::deserialize_put_result(&out).is_err());

        let delete_in = handle_sys_session::serialize_session_delete_param(1, b"alpha");
        let out = async_delete_internal(delete_in).await;
        assert!(handle_sys_session::deserialize_delete_result(&out).is_err());

        let range_in = handle_sys_session::serialize_session_range_param(1, b"a", b"z");
        let out = async_range_internal(range_in).await;
        assert!(handle_sys_session::deserialize_range_result(&out).is_err());
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
