#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::*;
use crate::utils::*;
use ::catalog::contract::message_bus_api::{
    set_current_message_bus, unset_current_message_bus, DeliveryMode, Envelope, MessageBus,
    MessageId, OutgoingMessage, RecvFilter, SubscriptionId,
};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
use ::xengine::tx_mgr::PhysicalRelationId;
use async_trait::async_trait;
use mudu_sys::env_var::temp_dir;
use mudu_sys::sync::SMutex;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use mudu_utils::oid::gen_oid;
use std::collections::VecDeque;
use std::sync::Arc;

#[path = "rpc_test/cross_partition.rs"]
mod cross_partition;
#[path = "rpc_test/locks_deltas.rs"]
mod locks_deltas;
#[path = "rpc_test/rpc_ops.rs"]
mod rpc_ops;

fn test_schema() -> ::catalog::contract::schema_table::SchemaTable {
    // Use a fixed OID so helper functions that rebuild the schema return the same table id.
    ::catalog::contract::schema_table::SchemaTable::new_with_oid(
        42,
        "t".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "v".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
        ],
        vec![0],
        vec![1],
    )
}

fn meta_table(schema: &::catalog::contract::schema_table::SchemaTable) -> RS<Arc<TableDesc>> {
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

struct MockMessageBus {
    local_endpoint: OID,
    sent: SMutex<Vec<(OID, OutgoingMessage)>>,
    responses: SMutex<VecDeque<Envelope>>,
}

impl MockMessageBus {
    fn new(local_endpoint: OID) -> Self {
        Self {
            local_endpoint,
            sent: SMutex::new(Vec::new()),
            responses: SMutex::new(VecDeque::new()),
        }
    }

    fn push_response(&self, envelope: Envelope) {
        self.responses.lock().unwrap().push_back(envelope);
    }

    fn take_sent(&self) -> Vec<(OID, OutgoingMessage)> {
        self.sent.lock().unwrap().drain(..).collect()
    }
}

#[async_trait]
impl MessageBus for MockMessageBus {
    fn local_endpoint(&self) -> OID {
        self.local_endpoint
    }

    async fn send(&self, dst: OID, message: OutgoingMessage) -> RS<MessageId> {
        self.sent.lock().unwrap().push((dst, message));
        Ok(1)
    }

    async fn recv(&self, filter: RecvFilter) -> RS<Envelope> {
        let response = {
            let mut responses = self.responses.lock().unwrap();
            let mut found = None;
            for (index, envelope) in responses.iter().enumerate() {
                if envelope.matches(&filter) {
                    found = Some(responses.remove(index).unwrap());
                    break;
                }
            }
            found
        };
        match response {
            Some(envelope) => Ok(envelope),
            // Block forever when no matching response is queued.
            None => std::future::pending().await,
        }
    }

    fn on_recv_callback(
        &self,
        _filter: RecvFilter,
        _callback: ::catalog::contract::message_bus_api::OnRecvCallback,
    ) -> RS<SubscriptionId> {
        unimplemented!()
    }

    fn cancel_callback(&self, _id: SubscriptionId) -> RS<bool> {
        unimplemented!()
    }
}

fn response_envelope(
    msg_id: MessageId,
    src: OID,
    dst: OID,
    response: PartitionRpcResponse,
) -> Envelope {
    Envelope::new(
        msg_id + 100,
        Some(msg_id),
        src,
        dst,
        PARTITION_RPC_RESPONSE_KIND,
        rmp_serde::to_vec(&response).unwrap(),
        DeliveryMode::Response,
    )
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
    bus: &Arc<MockMessageBus>,
    desc: &Arc<TableDesc>,
) -> Arc<dyn TxMgr> {
    let tx = coordinator.worker_begin_tx().unwrap();
    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))])),
    ));
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
