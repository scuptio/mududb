#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::*;
use crate::utils::*;
use ::catalog::contract::channel_kinds::PARTITION_RPC;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
use ::xengine::tx_mgr::PhysicalRelationId;
use async_trait::async_trait;
use mudu::channel::{DeliveryMode, Envelope, MessageId, MessageKind, OutgoingMessage, RecvFilter};
use mudu_sys::env_var::temp_dir;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;
use mudu_utils::channel::{
    set_current_channel, unset_current_channel, ChannelEndpoint, KindHandler,
};
use mudu_utils::oid::gen_oid;
use std::collections::VecDeque;
use std::sync::Arc;

#[path = "rpc_test/assignments.rs"]
mod assignments;
#[path = "rpc_test/cross_partition.rs"]
mod cross_partition;
#[path = "rpc_test/locks_deltas.rs"]
mod locks_deltas;
#[path = "rpc_test/migration.rs"]
mod migration;
#[path = "rpc_test/rpc_ops.rs"]
mod rpc_ops;

fn test_schema() -> ::catalog::contract::schema_class::SchemaClass {
    // Use a fixed OID so helper functions that rebuild the schema return the same table id.
    ::catalog::contract::schema_class::SchemaClass::new_with_oid(
        42,
        "t".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                mudu_type::data_type_codec::from_opt_object(&DataType::default_for(
                    TypeFamily::I32,
                )),
            ),
            SchemaColumn::new(
                "v".to_string(),
                TypeFamily::I32,
                mudu_type::data_type_codec::from_opt_object(&DataType::default_for(
                    TypeFamily::I32,
                )),
            ),
        ],
        vec![0],
        vec![1],
    )
}

fn meta_table(schema: &::catalog::contract::schema_class::SchemaClass) -> RS<Arc<TableDesc>> {
    ::catalog::contract::table_info::TableInfo::new(schema.clone())?.table_desc()
}

fn datum(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn key_row(v: i32) -> VecDatum {
    VecDatum::new(vec![(0, datum(v))])
}

fn value_row(v: i32) -> VecDatum {
    VecDatum::new(vec![(1, datum(v))])
}

async fn make_contract() -> WorkerXContract {
    make_contract_with_worker(0, 0).await
}

async fn make_contract_with_worker(
    worker_id: OID,
    default_unpartitioned_worker_id: OID,
) -> WorkerXContract {
    let data_dir = temp_dir()
        .join(format!("rpc_test_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
        meta_mgr: Arc::new(TestMetaMgr::new()),
        log: None,
        log_layout: Default::default(),
        active_sessions: Default::default(),
        worker_id,
        default_unpartitioned_worker_id,
        partition_id: 0,
        data_dir,
        async_runtime: None,
        server_instance_id: 0,
        cluster_router: None,
        epoch_oracle: None,
    })
    .unwrap();
    let schema = test_schema();
    let tx = contract.begin_tx().await.unwrap();
    contract.create_table(tx.clone(), &schema).await.unwrap();
    contract.commit_tx(tx).await.unwrap();
    contract
}

fn table_id() -> OID {
    test_schema().id()
}

/// Scripted reply to one request: `Reply` answers it, `Drop` loses it
/// (fault injection for the retry tests).
enum Responder {
    Reply(PartitionRpcResponse),
    Drop,
}

struct MockWorkerChannel {
    local_endpoint: OID,
    sent: SMutex<Vec<(OID, OutgoingMessage)>>,
    responders: SMutex<VecDeque<Responder>>,
    pending: SMutex<VecDeque<Envelope>>,
}

impl MockWorkerChannel {
    fn new(local_endpoint: OID) -> Self {
        Self {
            local_endpoint,
            sent: SMutex::new(Vec::new()),
            responders: SMutex::new(VecDeque::new()),
            pending: SMutex::new(VecDeque::new()),
        }
    }

    fn push_responder(&self, responder: Responder) {
        self.responders.lock().unwrap().push_back(responder);
    }

    fn take_sent(&self) -> Vec<(OID, OutgoingMessage)> {
        self.sent.lock().unwrap().drain(..).collect()
    }
}

#[async_trait]
impl ChannelEndpoint for MockWorkerChannel {
    fn local_endpoint(&self) -> OID {
        self.local_endpoint
    }

    async fn send(&self, dst: OID, message: OutgoingMessage) -> RS<MessageId> {
        let msg_id = (self.sent.lock().unwrap().len() + 1) as MessageId;
        if message.delivery() == DeliveryMode::Request {
            let responder = self.responders.lock().unwrap().pop_front();
            if let Some(Responder::Reply(response)) = responder {
                let payload = rmp_serde::to_vec(&response).unwrap();
                self.pending.lock().unwrap().push_back(
                    Envelope::new(
                        msg_id + 100,
                        Some(message.request_id()),
                        dst,
                        self.local_endpoint,
                        message.kind(),
                        payload,
                        DeliveryMode::Response,
                    )
                    .with_request_id(message.request_id()),
                );
            }
        }
        self.sent.lock().unwrap().push((dst, message));
        Ok(msg_id)
    }

    async fn recv(&self, filter: RecvFilter, _deadline_ms: Option<u64>) -> RS<Envelope> {
        let found = {
            let mut pending = self.pending.lock().unwrap();
            let mut found = None;
            for (index, envelope) in pending.iter().enumerate() {
                if envelope.matches(&filter) {
                    found = Some(pending.remove(index).unwrap());
                    break;
                }
            }
            found
        };
        match found {
            Some(envelope) => Ok(envelope),
            // Block forever when no matching response is queued.
            None => std::future::pending().await,
        }
    }

    fn register_kind_handler(&self, _kind: MessageKind, _handler: KindHandler) -> RS<()> {
        unimplemented!()
    }

    fn unregister_kind_handler(&self, _kind: MessageKind) -> RS<()> {
        unimplemented!()
    }
}

fn read_key_request() -> PartitionRpcRequest {
    PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: b"k".to_vec(),
        select: vec![],
    }
}

/// Seed key 1 with value 10 on `owner` via the Insert RPC.
async fn seed_owner_key1(owner: &WorkerXContract, desc: &Arc<TableDesc>) {
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), desc).unwrap(),
        value: build_value_tuple(&value_row(10), desc).unwrap(),
    };
    owner.execute_partition_rpc(seed).await.unwrap();
}

/// Run the coordinator's update on key 1: pre-queue the projected value the
/// owner would return for the `LockKeyForUpdate` round trip, then update.
/// Returns the coordinator transaction holding the remote lock on worker 20.
async fn coordinator_update_key1(
    coordinator: &WorkerXContract,
    bus: &Arc<MockWorkerChannel>,
    desc: &Arc<TableDesc>,
) -> Arc<dyn TxMgr> {
    let tx = coordinator.worker_begin_tx().unwrap();
    bus.push_responder(Responder::Reply(PartitionRpcResponse::ReadKey(Some(vec![
        Some(datum(10)),
    ]))));
    coordinator
        ._update(
            desc.clone(),
            tx.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![(1, datum(30))]),
            &[],
            &[],
        )
        .await
        .unwrap();
    assert_eq!(tx.remote_lock_owners(), vec![20]);
    tx
}
