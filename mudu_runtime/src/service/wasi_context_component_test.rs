#[cfg(test)]
mod tests {
    #![allow(clippy::unimplemented)]

    use super::super::wasi_context_component::{
        WasiContextComponent, build_wasi_component_context, sync_host,
    };
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu::common::serde_utils::deserialize_from;
    use mudu_binding::codec::handle_sys_session;
    use mudu_binding::system::{command_invoke, query_invoke};
    use mudu_binding::universal::uni_error::UniError;
    use mudu_contract::database::result_set::ResultSetAsync;
    use mudu_contract::database::sql_params::SQLParams;
    use mudu_contract::database::sql_stmt::SQLStmt;
    use mudu_kernel::contract::meta_mgr::MetaMgr;
    use mudu_kernel::server::message_bus_api::MessageBusRef;
    use mudu_kernel::server::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
    use mudu_kernel::server::worker_snapshot::KvItem;
    use mudu_kernel::x_engine::api::XContract;
    use std::sync::Arc;
    use sync_host::mududb::api::system::Host;
    use wasmtime_wasi::WasiView;

    const MERR_MAGIC: &[u8] = b"MERR";

    fn decode_merr_payload(bytes: &[u8]) -> UniError {
        assert!(bytes.starts_with(MERR_MAGIC));
        deserialize_from::<UniError>(&bytes[MERR_MAGIC.len()..])
            .map(|(e, _)| e)
            .expect("valid MERR payload")
    }

    fn assert_worker_local_error(bytes: &[u8]) {
        let err = decode_merr_payload(bytes);
        assert!(
            err.err_msg
                .contains("worker local interface is not configured"),
            "unexpected error message: {}",
            err.err_msg
        );
    }

    fn assert_no_session_error(bytes: &[u8]) {
        let err = query_invoke::deserialize_query_result(bytes)
            .err()
            .or_else(|| command_invoke::deserialize_command_result(bytes).err())
            .expect("result should be an error");
        assert!(
            err.to_string().contains("no such session id"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn build_wasi_component_context_without_worker_local() {
        let mut ctx = build_wasi_component_context(None);
        assert!(ctx.worker_local().is_none());
        let _wasi_view = ctx.ctx();
    }

    #[test]
    fn new_wasi_context_component_with_worker_local() {
        let dummy: WorkerLocalRef = Arc::new(DummyWorkerLocal);
        let ctx = WasiContextComponent::new(
            wasmtime_wasi::WasiCtxBuilder::new().build(),
            Some(dummy.clone()),
        );
        assert!(ctx.worker_local().is_some());
        assert!(Arc::ptr_eq(&ctx.worker_local().unwrap(), &dummy));
    }

    #[test]
    fn sync_host_query_without_session_returns_no_session_error() {
        let input = query_invoke::serialize_query_dyn_param(12345, &"SELECT 1", &())
            .expect("serialize query param");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.query(input);
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn sync_host_command_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.command(input);
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn sync_host_batch_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.batch(input);
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn sync_host_fetch_empty_returns_empty() {
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.fetch(vec![]);
        assert!(output.is_empty());
    }

    #[test]
    fn sync_host_open_without_worker_local_returns_worker_local_error() {
        let input = handle_sys_session::serialize_open_param();
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.open(input);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn sync_host_close_without_worker_local_returns_worker_local_error() {
        let input = handle_sys_session::serialize_close_param(1);
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.close(input);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn sync_host_get_without_worker_local_returns_worker_local_error() {
        let input = handle_sys_session::serialize_session_get_param(1, b"alpha");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.get(input);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn sync_host_put_without_worker_local_returns_worker_local_error() {
        let input = handle_sys_session::serialize_session_put_param(1, b"alpha", b"beta");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.put(input);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn sync_host_delete_without_worker_local_returns_worker_local_error() {
        let input = handle_sys_session::serialize_session_delete_param(1, b"alpha");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.delete(input);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn sync_host_range_without_worker_local_returns_worker_local_error() {
        let input = handle_sys_session::serialize_session_range_param(1, b"a", b"z");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.range(input);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn sync_host_query_malformed_input_does_not_panic() {
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.query(vec![0xff, 0x00, 0xab, 0xcd]);
        assert!(!output.is_empty());
        assert!(query_invoke::deserialize_query_result(&output).is_err());
    }

    struct DummyWorkerLocal;

    #[async_trait]
    impl WorkerLocal for DummyWorkerLocal {
        fn x_contract(&self) -> Arc<dyn XContract> {
            unimplemented!()
        }

        fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
            unimplemented!()
        }

        fn message_bus(&self) -> MessageBusRef {
            unimplemented!()
        }

        async fn open_async(&self) -> RS<OID> {
            unimplemented!()
        }

        async fn close_async(&self, _session_id: OID) -> RS<()> {
            unimplemented!()
        }

        async fn execute_async(&self, _session_id: OID, _instruction: WorkerExecute) -> RS<()> {
            unimplemented!()
        }

        async fn put_async(&self, _session_id: OID, _key: Vec<u8>, _value: Vec<u8>) -> RS<()> {
            unimplemented!()
        }

        async fn delete_async(&self, _session_id: OID, _key: &[u8]) -> RS<()> {
            unimplemented!()
        }

        async fn get_async(&self, _session_id: OID, _key: &[u8]) -> RS<Option<Vec<u8>>> {
            unimplemented!()
        }

        async fn range_async(
            &self,
            _session_id: OID,
            _start_key: &[u8],
            _end_key: &[u8],
        ) -> RS<Vec<KvItem>> {
            unimplemented!()
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
}
