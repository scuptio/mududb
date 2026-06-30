#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::HandshakeHandler;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::server::async_func_task::HandleResult;
use crate::server::message_bus_api::{
    Envelope, MessageBus, MessageBusRef, MessageId, OnRecvCallback, OutgoingMessage, RecvFilter,
    SubscriptionId,
};
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;
use crate::server::request_response_worker::{RequestResponseWorker, WorkerRuntimeRef};
use crate::server::routing::SessionOpenConfig;
use crate::server::worker_local::{WorkerExecute, WorkerLocal};
use crate::server::worker_registry::WorkerIdentity;
use crate::server::worker_registry::WorkerRegistry;
use crate::server::worker_snapshot::KvItem;
use crate::x_engine::api::{
    AlterTable, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, VecDatum,
    VecSelTerm, XContract,
};
use crate::x_engine::tx_mgr::TxMgr;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::protocol::{
    decode_handshake_response, encode_handshake_request, Frame, HandshakeRequest,
    HandshakeResponse, MessageType,
};
use std::sync::Arc;

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

struct NullWorkerRuntime;

#[async_trait]
impl WorkerLocal for NullWorkerRuntime {
    fn x_contract(&self) -> Arc<dyn XContract> {
        Arc::new(NullXContract)
    }
    fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        Arc::new(NullMetaMgr)
    }
    fn message_bus(&self) -> MessageBusRef {
        Arc::new(NullMessageBus)
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

#[async_trait]
impl RequestResponseWorker for NullWorkerRuntime {
    fn worker_index(&self) -> usize {
        0
    }
    fn worker_id(&self) -> OID {
        0
    }
    fn registry(&self) -> Arc<WorkerRegistry> {
        Arc::new(
            WorkerRegistry::new(vec![WorkerIdentity {
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

fn null_ctx(request_id: u64) -> RequestCtx {
    RequestCtx::new(
        Arc::new(NullWorkerRuntime) as WorkerRuntimeRef,
        0,
        request_id,
    )
}

fn decode_response(bytes: &[u8]) -> HandshakeResponse {
    let frame = Frame::decode(bytes).unwrap();
    decode_handshake_response(&frame).unwrap()
}

#[test]
fn handshake_handler_message_type_is_handshake() {
    let handler = HandshakeHandler;
    assert_eq!(handler.message_type(), MessageType::Handshake);
}

#[test]
fn handshake_handler_valid_request_returns_response() {
    futures::executor::block_on(async {
        let request_id = 12345u64;
        let request = HandshakeRequest {
            supported_versions: vec![1],
            capabilities: vec![],
        };
        let frame_bytes = encode_handshake_request(request_id, &request).unwrap();
        let frame = Frame::decode(&frame_bytes).unwrap();
        assert_eq!(frame.header().message_type(), MessageType::Handshake);

        let ctx = null_ctx(request_id);
        let handler = HandshakeHandler;
        let result = handler.handle(&ctx, &frame).await.unwrap();
        let HandleResult::Response(response_bytes) = result;

        let response = decode_response(&response_bytes);
        assert_eq!(response.selected_version, 1);
        assert!(response
            .capabilities
            .contains(&"protocol.handshake".to_string()));
        assert!(response
            .capabilities
            .contains(&"result.table.v1".to_string()));

        let response_frame = Frame::decode(&response_bytes).unwrap();
        assert_eq!(response_frame.header().request_id(), request_id);
    });
}

#[test]
fn handshake_handler_empty_versions_returns_parse_error() {
    futures::executor::block_on(async {
        let request = HandshakeRequest {
            supported_versions: vec![],
            capabilities: vec![],
        };
        let frame_bytes = encode_handshake_request(1, &request).unwrap();
        let frame = Frame::decode(&frame_bytes).unwrap();
        let ctx = null_ctx(1);
        let handler = HandshakeHandler;
        let result = handler.handle(&ctx, &frame).await;
        match result {
            Err(err) => assert_eq!(err.ec().to_u32(), mudu::error::ErrorCode::Parse.to_u32()),
            Ok(_) => panic!("expected parse error"),
        }
    });
}

#[test]
fn handshake_handler_unsupported_version_returns_parse_error() {
    for versions in [vec![2], vec![0], vec![1, 2]] {
        futures::executor::block_on(async {
            let request = HandshakeRequest {
                supported_versions: versions,
                capabilities: vec![],
            };
            let frame_bytes = encode_handshake_request(1, &request).unwrap();
            let frame = Frame::decode(&frame_bytes).unwrap();
            let ctx = null_ctx(1);
            let handler = HandshakeHandler;
            let result = handler.handle(&ctx, &frame).await;
            match result {
                Err(err) => assert_eq!(err.ec().to_u32(), mudu::error::ErrorCode::Parse.to_u32()),
                Ok(_) => panic!("expected parse error"),
            }
        });
    }
}

#[test]
fn handshake_handler_malformed_frame_returns_decode_error() {
    futures::executor::block_on(async {
        let payload = b"not a valid msgpack payload";
        let frame = Frame::new(MessageType::Handshake, 1, payload.to_vec());
        let ctx = null_ctx(1);
        let handler = HandshakeHandler;
        let result = handler.handle(&ctx, &frame).await;
        match result {
            Err(err) => assert_eq!(err.ec().to_u32(), mudu::error::ErrorCode::Decode.to_u32()),
            Ok(_) => panic!("expected decode error"),
        }
    });
}
