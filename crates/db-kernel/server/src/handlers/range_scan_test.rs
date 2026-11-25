#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::handshake_test::{NullMetaMgr, NullWorkerChannel, NullWorkerRuntime, NullXContract};
use super::{MessageHandler, RangeScanHandler};
use crate::async_func_task::HandleResult;
use crate::request_ctx::RequestCtx;
use crate::request_response_worker::{RequestResponseWorker, WorkerRuntimeRef};
use crate::routing::SessionOpenConfig;
use crate::worker_registry::{WorkerIdobject, WorkerRegistry};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::worker_snapshot::KvItem;
use ::conn::worker_local::{WorkerExecute, WorkerLocal};
use ::xengine::api::XContract;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::protocol::{
    decode_range_scan_response, encode_range_scan_request, Frame, MessageType, RangeScanRequest,
};
use mudu_utils::channel::ChannelEndpointRef;
use std::sync::Arc;

/// A worker stub whose `range_async` serves fixed items; every other entry
/// point delegates to the null fixtures (never reached by the range scan
/// flow).
struct RangeWorker(NullWorkerRuntime);

#[async_trait]
impl WorkerLocal for RangeWorker {
    fn x_contract(&self) -> Arc<dyn XContract> {
        Arc::new(NullXContract)
    }
    fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        Arc::new(NullMetaMgr)
    }
    fn worker_channel(&self) -> ChannelEndpointRef {
        Arc::new(NullWorkerChannel)
    }
    async fn open_async(&self) -> RS<OID> {
        unimplemented!()
    }
    async fn open_argv_async(&self, _worker_id: OID) -> RS<OID> {
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
        Ok(vec![
            KvItem {
                key: b"a".to_vec(),
                value: b"1".to_vec(),
            },
            KvItem {
                key: b"b".to_vec(),
                value: b"2".to_vec(),
            },
        ])
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

#[async_trait]
impl RequestResponseWorker for RangeWorker {
    fn worker_index(&self) -> usize {
        0
    }
    fn worker_id(&self) -> OID {
        0
    }
    fn registry(&self) -> Arc<WorkerRegistry> {
        Arc::new(
            WorkerRegistry::new(vec![WorkerIdobject {
                worker_index: 0,
                worker_id: 0,
                partition_ids: vec![],
            }])
            .unwrap(),
        )
    }
    fn open_session_with_config(&self, _conn_id: u64, _config: SessionOpenConfig) -> RS<OID> {
        unimplemented!()
    }
    fn close_session_for_connection(&self, _conn_id: u64, _session_id: OID) -> RS<bool> {
        unimplemented!()
    }
    async fn handle_procedure_request(
        &self,
        _conn_id: u64,
        _request: &mudu_contract::protocol::ProcedureInvokeRequest,
    ) -> RS<mudu_contract::protocol::ProcedureInvokeResponse> {
        unimplemented!()
    }
}

fn range_ctx(request_id: u64) -> RequestCtx {
    RequestCtx::new(
        Arc::new(RangeWorker(NullWorkerRuntime)) as WorkerRuntimeRef,
        0,
        request_id,
    )
}

#[test]
fn range_scan_handler_message_type_is_range_scan() {
    assert_eq!(RangeScanHandler.message_type(), MessageType::RangeScan);
}

#[test]
fn range_scan_valid_request_returns_scanned_items() {
    futures::executor::block_on(async {
        let request_id = 77u64;
        let request = RangeScanRequest::new(9, b"a".to_vec(), b"z".to_vec());
        let frame_bytes = encode_range_scan_request(request_id, &request).unwrap();
        let frame = Frame::decode(&frame_bytes).unwrap();

        let ctx = range_ctx(request_id);
        let result = RangeScanHandler.handle(&ctx, &frame).await.unwrap();
        let HandleResult::Response(response_bytes) = result;

        let response_frame = Frame::decode(&response_bytes).unwrap();
        assert_eq!(response_frame.header().request_id(), request_id);
        let response = decode_range_scan_response(&response_frame).unwrap();
        let items = response.items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key(), b"a");
        assert_eq!(items[0].value(), b"1");
        assert_eq!(items[1].key(), b"b");
        assert_eq!(items[1].value(), b"2");
    });
}

#[test]
fn range_scan_malformed_frame_returns_decode_error() {
    futures::executor::block_on(async {
        let frame = Frame::new(MessageType::RangeScan, 1, b"garbage".to_vec());
        let ctx = range_ctx(1);
        let result = RangeScanHandler.handle(&ctx, &frame).await;
        match result {
            Err(err) => assert_eq!(err.ec().to_u32(), mudu::error::ErrorCode::Decode.to_u32()),
            Ok(_) => panic!("expected decode error"),
        }
    });
}

/// The `RangeWorker` entry points outside `range_async` satisfy trait bounds
/// the range scan flow never calls; pin that contract down by driving every
/// one of them and asserting each panics (or returns its documented value).
#[test]
fn range_worker_fixture_covers_its_surface() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    fn panics<R>(f: impl FnOnce() -> R) {
        assert!(catch_unwind(AssertUnwindSafe(f)).is_err());
    }
    fn panics_async<F: std::future::Future>(fut: F) {
        panics(move || futures::executor::block_on(fut));
    }

    let worker = RangeWorker(NullWorkerRuntime);
    assert_eq!(worker.worker_index(), 0);
    assert_eq!(worker.worker_id(), 0);
    let _registry = worker.registry();
    let _x = worker.x_contract();
    let _meta = worker.meta_mgr();
    let _bus = worker.worker_channel();

    panics_async(worker.open_async());
    panics_async(worker.open_argv_async(1));
    panics_async(worker.close_async(1));
    panics_async(worker.put_async(1, b"k".to_vec(), b"v".to_vec()));
    panics_async(worker.delete_async(1, b"k"));
    panics_async(worker.get_async(1, b"k"));
    let new_stmt = |sql: &str| {
        Box::new(mudu_contract::database::sql_stmt_text::SQLStmtText::new(
            sql.to_string(),
        )) as Box<dyn SQLStmt>
    };
    panics_async(worker.query(1, None, new_stmt("select 1"), Box::new(())));
    panics_async(worker.execute(1, None, new_stmt("update t set x = 1"), Box::new(())));
    panics_async(worker.batch(1, None, new_stmt("insert into t values (1)"), Box::new(())));

    panics(|| worker.close_session_for_connection(1, 2));
    panics_async(worker.handle_procedure_request(
        0,
        &mudu_contract::protocol::ProcedureInvokeRequest::new(1, "p".to_string(), vec![]),
    ));
}
