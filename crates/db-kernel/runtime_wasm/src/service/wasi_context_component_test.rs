#[cfg(test)]
mod tests {
    #![allow(clippy::unimplemented)]

    use super::super::wasi_context_component::{
        WasiContextComponent, build_wasi_component_context, sync_host,
    };
    use ::catalog::contract::meta_mgr::MetaMgr;
    use ::catalog::contract::worker_snapshot::KvItem;
    use ::conn::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
    use ::xengine::api::XContract;
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu_binding::codec::syscall_payload::{
        decode_close_result, decode_delete_result, decode_get_result, decode_open_result,
        decode_put_result, decode_range_result, encode_close_request, encode_delete_request,
        encode_get_request, encode_open_request, encode_put_request, encode_range_request,
    };
    use mudu_binding::system::{command_invoke, query_invoke};
    use mudu_binding::universal::mp_wire::{FromValue, decode_value};
    use mudu_binding::universal::uni_error::UniError;
    use mudu_binding::universal::uni_oid::UniOid;
    use mudu_binding::universal::uni_result::UniResult;
    use mudu_binding::universal::uni_result_set::UniResultSet;
    use mudu_contract::database::result_set::ResultSetAsync;
    use mudu_contract::database::sql_params::SQLParams;
    use mudu_contract::database::sql_stmt::SQLStmt;
    use mudu_utils::channel::ChannelEndpointRef;
    use std::sync::Arc;
    use sync_host::mududb::api::system::Host;
    use wasmtime_wasi::WasiView;

    fn assert_worker_local_error(message: &str) {
        assert!(
            message.contains("worker local interface is not configured"),
            "unexpected error message: {message}"
        );
    }

    fn assert_no_session_error(bytes: &[u8]) {
        // The output is a command-kind MSSP frame (the batch handler shares
        // the command result serializer); decode it with the matching kind.
        let err = command_invoke::deserialize_command_result(bytes)
            .expect_err("result should be an error");
        assert!(
            err.to_string().contains("no such session id"),
            "unexpected error: {}",
            err
        );
    }

    fn assert_no_session_query_error(bytes: &[u8]) {
        let err = query_invoke::deserialize_query_result(bytes)
            .err()
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
        let output = ctx.query(input).unwrap();
        assert!(!output.is_empty());
        assert_no_session_query_error(&output);
    }

    #[test]
    fn sync_host_command_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.command(input).unwrap();
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn sync_host_batch_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.batch(input).unwrap();
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn sync_host_fetch_malformed_cursor_returns_uni_error() {
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.fetch(vec![]).unwrap();
        assert!(!output.is_empty());
        // The empty cursor fails to decode as a `UniOid`, so the handler
        // returns the failure inside the `UniResult::Err` envelope.
        let result: UniResult<UniResultSet, UniError> =
            UniResult::from_value(&decode_value(&output).expect("fetch response should decode"))
                .expect("fetch response should convert");
        assert!(
            matches!(result, UniResult::Err(_)),
            "malformed cursor should yield a UniResult::Err envelope"
        );
    }

    #[test]
    fn sync_host_open_without_worker_local_returns_worker_local_error() {
        let input = encode_open_request(UniOid::from_oid(0));
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.open(input).unwrap();
        let err = decode_open_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    #[test]
    fn sync_host_close_without_worker_local_returns_worker_local_error() {
        // Close falls back to the raw open-minted-session path when no task
        // context is registered; that path needs the worker local interface.
        let input = encode_close_request(UniOid::from_oid(1));
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.close(input).unwrap();
        let err = decode_close_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    #[test]
    fn sync_host_get_without_task_context_returns_no_session_error() {
        let input = encode_get_request(UniOid::from_oid(1), b"alpha");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.get(input).unwrap();
        let err = decode_get_result(&output).unwrap_err();
        assert!(err.message().contains("no such session id"));
    }

    #[test]
    fn sync_host_put_without_task_context_returns_no_session_error() {
        let input = encode_put_request(UniOid::from_oid(1), b"alpha", b"beta");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.put(input).unwrap();
        let err = decode_put_result(&output).unwrap_err();
        assert!(err.message().contains("no such session id"));
    }

    #[test]
    fn sync_host_delete_without_task_context_returns_no_session_error() {
        let input = encode_delete_request(UniOid::from_oid(1), b"alpha");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.delete(input).unwrap();
        let err = decode_delete_result(&output).unwrap_err();
        assert!(err.message().contains("no such session id"));
    }

    #[test]
    fn sync_host_range_without_task_context_returns_no_session_error() {
        let input = encode_range_request(UniOid::from_oid(1), b"a", b"z");
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.range(input).unwrap();
        let err = decode_range_result(&output).unwrap_err();
        assert!(err.message().contains("no such session id"));
    }

    #[test]
    fn sync_host_query_malformed_input_does_not_panic() {
        let mut ctx = build_wasi_component_context(None);
        let output = ctx.query(vec![0xff, 0x00, 0xab, 0xcd]).unwrap();
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

        fn worker_channel(&self) -> ChannelEndpointRef {
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
    #[test]
    fn sync_host_relation_methods_round_trip() {
        use crate::interface::kernel_test::{relation_context, relation_worker_local};
        use mudu_binding::codec::syscall_payload::{
            decode_relation_get_result, decode_relation_insert_result,
            decode_relation_update_result, encode_relation_get_request,
            encode_relation_insert_request, encode_relation_update_request,
        };
        use mudu_contract::database::sql::Context;

        let sid = UniOid::from_oid(91);
        let wl = relation_worker_local();
        relation_context(sid.to_oid(), wl.clone());
        let mut ctx = build_wasi_component_context(Some(wl));

        let out = ctx
            .relation_insert(encode_relation_insert_request(
                sid.clone(),
                "t",
                &[(0, &b"k"[..])],
                &[(1, &b"v"[..])],
            ))
            .unwrap();
        decode_relation_insert_result(&out).unwrap();
        let out = ctx
            .relation_get(encode_relation_get_request(
                sid.clone(),
                "t",
                &[(0, &b"k"[..])],
                &[1],
            ))
            .unwrap();
        assert_eq!(
            decode_relation_get_result(&out).unwrap(),
            Some(vec![Some(b"v".to_vec())])
        );
        let out = ctx
            .relation_update(encode_relation_update_request(
                sid.clone(),
                "t",
                &[(0, &b"k"[..])],
                &[(1, &b"w"[..])],
                &[],
            ))
            .unwrap();
        assert_eq!(decode_relation_update_result(&out).unwrap(), 1);
        Context::remove(sid.to_oid());
    }

    #[test]
    fn sync_host_fs_methods_round_trip() {
        use crate::interface::kernel_test::{fs_worker_local, task_context};
        use mudu_binding::codec::syscall_payload::{
            decode_fs_close_result, decode_fs_fstat_result, decode_fs_fsync_result,
            decode_fs_lseek_result, decode_fs_open_result, decode_fs_pread_result,
            decode_fs_pwrite_result, decode_fs_read_result, decode_fs_readdir_result,
            decode_fs_stat_result, decode_fs_write_result, encode_fs_close_request,
            encode_fs_fstat_request, encode_fs_fsync_request, encode_fs_lseek_request,
            encode_fs_open_request, encode_fs_pread_request, encode_fs_pwrite_request,
            encode_fs_read_request, encode_fs_readdir_request, encode_fs_stat_request,
            encode_fs_write_request,
        };
        use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
        use mudu_contract::database::sql::Context;

        let sid = UniOid::from_oid(92);
        let wl = fs_worker_local();
        task_context(sid.to_oid(), wl.clone());
        let mut ctx = build_wasi_component_context(Some(wl));

        let out = ctx
            .fs_open(encode_fs_open_request(&UniFsOpenArgv {
                session: sid.clone(),
                oid: UniOid::from_oid(2),
                path: "data.bin".to_string(),
                flags: 0,
            }))
            .unwrap();
        let fd = decode_fs_open_result(&out).unwrap();
        let out = ctx.fs_write(encode_fs_write_request(fd, b"AB")).unwrap();
        assert_eq!(decode_fs_write_result(&out).unwrap(), 2);
        let out = ctx.fs_lseek(encode_fs_lseek_request(fd, 0, 0)).unwrap();
        assert_eq!(decode_fs_lseek_result(&out).unwrap(), 0);
        let out = ctx.fs_read(encode_fs_read_request(fd, 8)).unwrap();
        assert_eq!(decode_fs_read_result(&out).unwrap(), b"ABllo-fs".to_vec());
        let out = ctx.fs_pread(encode_fs_pread_request(fd, 0, 2)).unwrap();
        assert_eq!(decode_fs_pread_result(&out).unwrap(), b"AB".to_vec());
        let out = ctx
            .fs_pwrite(encode_fs_pwrite_request(fd, 0, b"zz"))
            .unwrap();
        decode_fs_pwrite_result(&out).unwrap();
        let out = ctx.fs_fstat(encode_fs_fstat_request(fd)).unwrap();
        assert_eq!(decode_fs_fstat_result(&out).unwrap().length, 8);
        let out = ctx.fs_fsync(encode_fs_fsync_request(fd)).unwrap();
        decode_fs_fsync_result(&out).unwrap();
        let out = ctx
            .fs_stat(encode_fs_stat_request(UniOid::from_oid(2), "data.bin"))
            .unwrap();
        assert_eq!(decode_fs_stat_result(&out).unwrap().length, 8);
        let out = ctx
            .fs_readdir(encode_fs_readdir_request(UniOid::from_oid(2), "/"))
            .unwrap();
        assert_eq!(decode_fs_readdir_result(&out).unwrap().len(), 1);
        let out = ctx.fs_close(encode_fs_close_request(fd)).unwrap();
        decode_fs_close_result(&out).unwrap();
        Context::remove(sid.to_oid());
    }

    #[test]
    fn sync_host_open_close_track_resources() {
        use crate::interface::kernel_test::worker_local;

        let mut ctx = build_wasi_component_context(Some(worker_local()));
        let out = ctx.open(encode_open_request(UniOid::from_oid(0))).unwrap();
        let session = decode_open_result(&out).unwrap();
        assert_ne!(session, 0);
        let out = ctx
            .close(encode_close_request(UniOid::from_oid(session)))
            .unwrap();
        decode_close_result(&out).unwrap();
    }
}

#[cfg(test)]
mod async_drive_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::super::wasi_context_component::async_host::{drive_plain, drive_tracked};
    use crate::interface::kernel_test::{fs_worker_local, task_context, worker_local};
    use crate::service::invocation_limits::InvocationContext;
    use mudu_binding::codec::syscall_payload::{
        decode_fs_open_result, decode_open_result, decode_put_result, encode_fs_open_request,
        encode_open_request, encode_put_request,
    };
    use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
    use mudu_binding::universal::uni_oid::UniOid;
    use mudu_contract::database::sql::Context;

    #[tokio::test]
    async fn drive_plain_prepares_calls_and_leaves() {
        let control = InvocationContext::new(Default::default()).unwrap();
        // The handler runs inside the budget window: put through the KV mock.
        let wl = worker_local();
        let sid = UniOid::from_oid(8_200_201);
        task_context(sid.to_oid(), wl);
        let out = drive_plain(
            &control,
            "put",
            encode_put_request(sid.clone(), b"k", b"v"),
            None,
            |i, w, _c| async move {
                crate::service::kernel_function_p2_async::async_host_put(i, w).await
            },
        )
        .await
        .unwrap();
        decode_put_result(&out).unwrap();
        Context::remove(sid.to_oid());
    }

    #[tokio::test]
    async fn drive_plain_rejects_over_budget_reads() {
        let control = InvocationContext::new(Default::default()).unwrap();
        let over = mudu_binding::codec::syscall_payload::encode_fs_read_request(
            3,
            (control.limits.max_output_bytes + 1) as u32,
        );
        let err = drive_plain(&control, "fs_read", over, None, |_i, _w, _c| async move {
            Vec::new()
        })
        .await
        .unwrap_err();
        assert!(err.to_string().contains("output budget"));
    }

    #[tokio::test]
    async fn drive_tracked_records_opens_for_cleanup() {
        let control = InvocationContext::new(Default::default()).unwrap();
        let wl = fs_worker_local();
        task_context(1, wl.clone());

        let out = drive_tracked(
            &control,
            "open",
            encode_open_request(UniOid::from_oid(0)),
            Some(wl.clone()),
            |i, w, _c| async move {
                crate::service::kernel_function_p2_async::async_host_open(i, w).await
            },
        )
        .await
        .unwrap();
        assert!(decode_open_result(&out).is_ok());

        let out = drive_tracked(
            &control,
            "fs_open",
            encode_fs_open_request(&UniFsOpenArgv {
                session: UniOid::from_oid(1),
                oid: UniOid::from_oid(2),
                path: "data.bin".to_string(),
                flags: 0,
            }),
            Some(wl.clone()),
            |i, w, _c| async move {
                crate::service::kernel_function_p2_async::async_host_fs_open(i, w).await
            },
        )
        .await
        .unwrap();
        assert!(decode_fs_open_result(&out).is_ok());
        {
            let resources = control.resources.lock().unwrap();
            assert_eq!(resources.sessions.len(), 1);
            assert_eq!(resources.files.len(), 1);
        }
        // Cleanup closes both tracked handles without error.
        control.cleanup_resources().await.unwrap();
        Context::remove(1);
    }
}
