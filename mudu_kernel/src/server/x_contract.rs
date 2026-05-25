use async_trait::async_trait;
use futures::executor::block_on;
use mudu::common::buf::Buf;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::tuple::build_tuple::build_tuple;
use mudu_contract::tuple::tuple_binary::TupleBinary as TupleRaw;
use mudu_contract::tuple::update_tuple::update_tuple;
use mudu_type::dt_function::send_binary;
use mudu_utils::task_trace;
use std::ops::Bound;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, trace};

use crate::async_rt::contract::AsyncRuntime;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::meta::meta_mgr_factory::MetaMgrFactory;
use crate::server::message_bus_api::{
    current_message_bus, DeliveryMode, EndpointId, Envelope, MessageKind, OutgoingMessage,
    RecvFilter, ServerInstanceId,
};
use crate::server::partition_router::{PartitionRouter, DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID};
use crate::server::partition_rpc::{PartitionRpcRequest, PartitionRpcResponse, RpcBound};
use crate::server::worker_snapshot::{KvItem, WorkerSnapshot, WorkerSnapshotMgr};
use crate::server::worker_storage::WorkerStorage;
use crate::server::worker_tx_manager::WorkerTxManager;
use crate::server::x_lock_mgr::XLockMgr;
use crate::wal::worker_log::ChunkedWorkerLogBackend;
use crate::wal::xl_batch::{new_xl_batch_writer, XLBatch};
use crate::x_engine::api::{
    AlterTable, Filter, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData,
    TupleRow, VecDatum, VecSelTerm, XContract,
};
use crate::x_engine::tx_mgr::TxMgr;
type DatBin = Buf;
const PARTITION_RPC_REQUEST_KIND: MessageKind = MessageKind::User(0x7101);
const PARTITION_RPC_RESPONSE_KIND: MessageKind = MessageKind::User(0x7102);

pub struct WorkerXContract {
    server_instance_id: ServerInstanceId,
    worker_id: OID,
    default_unpartitioned_worker_id: OID,
    meta_mgr: Arc<dyn MetaMgr>,
    storage: Arc<WorkerStorage>,
    partition_router: PartitionRouter,
    partition_rpc_registered: AtomicBool,
    log: Option<ChunkedWorkerLogBackend>,
    snapshot_mgr: WorkerSnapshotMgr,
    tx_lock: XLockMgr,
    // commit_gate: AsyncMutex<()>,
}

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only contract.
pub type IoUringXContract = WorkerXContract;

struct VecCursor {
    inner: Mutex<VecCursorInner>,
}

struct VecCursorInner {
    rows: Vec<TupleRow>,
    index: usize,
}

impl WorkerXContract {
    pub fn new(meta_mgr: Arc<dyn MetaMgr>) -> RS<Self> {
        Self::with_log_and_data_dir(meta_mgr, None, 0, 0, 0, default_worker_storage_data_dir())
    }

    pub fn with_log(meta_mgr: Arc<dyn MetaMgr>, log: Option<ChunkedWorkerLogBackend>) -> RS<Self> {
        Self::with_log_and_data_dir(meta_mgr, log, 0, 0, 0, default_worker_storage_data_dir())
    }

    pub fn with_log_and_data_dir(
        meta_mgr: Arc<dyn MetaMgr>,
        log: Option<ChunkedWorkerLogBackend>,
        worker_id: OID,
        default_unpartitioned_worker_id: OID,
        partition_id: OID,
        data_dir: String,
    ) -> RS<Self> {
        Self::with_log_and_data_dir_and_runtime(
            meta_mgr,
            log,
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
            None,
            0,
        )
    }

    pub fn with_log_and_data_dir_and_runtime(
        meta_mgr: Arc<dyn MetaMgr>,
        log: Option<ChunkedWorkerLogBackend>,
        worker_id: OID,
        default_unpartitioned_worker_id: OID,
        partition_id: OID,
        data_dir: String,
        async_runtime: Option<Arc<dyn AsyncRuntime>>,
        server_instance_id: ServerInstanceId,
    ) -> RS<Self> {
        let storage = Arc::new(WorkerStorage::new_with_async_runtime(
            meta_mgr.clone(),
            partition_id,
            data_dir,
            async_runtime,
        ));
        storage.register_global();
        bootstrap_storage(&storage)?;
        Ok(Self {
            server_instance_id,
            worker_id,
            default_unpartitioned_worker_id,
            meta_mgr: meta_mgr.clone(),
            storage,
            partition_router: PartitionRouter::new(meta_mgr.clone()),
            partition_rpc_registered: AtomicBool::new(false),
            log,
            snapshot_mgr: WorkerSnapshotMgr::default(),
            tx_lock: XLockMgr::new(),
        })
    }

    pub fn with_worker_log(log: ChunkedWorkerLogBackend) -> RS<Self> {
        Self::with_worker_log_and_data_dir(log, 0, 0, 0, default_worker_storage_data_dir())
    }

    pub fn with_worker_log_and_data_dir(
        log: ChunkedWorkerLogBackend,
        worker_id: OID,
        default_unpartitioned_worker_id: OID,
        partition_id: OID,
        data_dir: String,
    ) -> RS<Self> {
        let meta_mgr = MetaMgrFactory::create(data_dir.clone())
            .map_err(|e| m_error!(EC::DBInternalError, "create worker meta manager failed", e))?;
        Self::with_log_and_data_dir(
            meta_mgr,
            Some(log.clone()),
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
        )
    }

    pub fn with_worker_log_and_data_dir_and_runtime(
        log: ChunkedWorkerLogBackend,
        worker_id: OID,
        default_unpartitioned_worker_id: OID,
        partition_id: OID,
        data_dir: String,
        async_runtime: Option<Arc<dyn AsyncRuntime>>,
        server_instance_id: ServerInstanceId,
    ) -> RS<Self> {
        let meta_mgr = MetaMgrFactory::create(data_dir.clone())
            .map_err(|e| m_error!(EC::DBInternalError, "create worker meta manager failed", e))?;
        Self::with_log_and_data_dir_and_runtime(
            meta_mgr,
            Some(log.clone()),
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
            async_runtime,
            server_instance_id,
        )
    }

    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

    pub fn worker_log(&self) -> Option<ChunkedWorkerLogBackend> {
        self.log.clone()
    }

    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    async fn resolve_partition_worker(&self, partition_id: OID) -> RS<Option<OID>> {
        match self.meta_mgr.get_partition_worker(partition_id).await? {
            Some(worker_id) => Ok(Some(worker_id)),
            None if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID => {
                Ok((self.default_unpartitioned_worker_id != 0)
                    .then_some(self.default_unpartitioned_worker_id))
            }
            None => Ok(None),
        }
    }

    pub fn worker_begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(WorkerTxManager::new(self.snapshot_mgr.begin_tx())))
    }

    pub fn worker_rollback_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        self.snapshot_mgr.end_tx(tx_mgr.xid())
    }

    pub fn worker_put(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let prepared = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            (
                self.storage.clone(),
                self.log.clone(),
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    Some(value.clone()),
                    single_put_batch(xid, key, value),
                ),
            )
        };
        let (storage, log, prepared) = prepared;
        if let Some(log) = log {
            new_xl_batch_writer(log).append_sync(prepared.batch())?;
        }
        storage.apply_prepared_commit(prepared)
    }

    pub async fn worker_put_async(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            (
                self.storage.clone(),
                self.log.clone(),
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    Some(value.clone()),
                    single_put_batch(xid, key, value),
                ),
            )
        };
        if let Some(log) = log {
            new_xl_batch_writer(log).append(prepared.batch()).await?;
        }
        storage.apply_prepared_commit_async(prepared).await
    }

    pub fn worker_delete(&self, key: &[u8]) -> RS<()> {
        let key = key.to_vec();
        let prepared = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            (
                self.storage.clone(),
                self.log.clone(),
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    None,
                    single_delete_batch(xid, key),
                ),
            )
        };
        let (storage, log, prepared) = prepared;
        if let Some(log) = log {
            new_xl_batch_writer(log).append_sync(prepared.batch())?;
        }
        storage.apply_prepared_commit(prepared)
    }

    pub async fn worker_delete_async(&self, key: &[u8]) -> RS<()> {
        let key = key.to_vec();
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            (
                self.storage.clone(),
                self.log.clone(),
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    None,
                    single_delete_batch(xid, key),
                ),
            )
        };
        if let Some(log) = log {
            new_xl_batch_writer(log).append(prepared.batch()).await?;
        }
        storage.apply_prepared_commit_async(prepared).await
    }

    pub async fn worker_get_async(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, None).await
    }

    pub async fn worker_get_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, Some(snapshot)).await
    }

    pub fn worker_get(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        block_on(self.storage.kv_get(key, None))
    }

    pub fn worker_get_with_snapshot(
        &self,
        snapshot: &WorkerSnapshot,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        block_on(self.storage.kv_get(key, Some(snapshot)))
    }

    pub fn worker_range_scan(&self, start_key: &[u8], end_key: &[u8]) -> RS<Vec<KvItem>> {
        block_on(self.storage.kv_range(start_key, end_key, None))
    }

    pub async fn worker_range_scan_async(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage.kv_range(start_key, end_key, None).await
    }

    pub fn worker_range_scan_with_snapshot(
        &self,
        snapshot: &WorkerSnapshot,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        block_on(self.storage.kv_range(start_key, end_key, Some(snapshot)))
    }

    pub async fn worker_range_scan_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage
            .kv_range(start_key, end_key, Some(snapshot))
            .await
    }

    pub fn worker_commit_put_batch(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: std::collections::BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        if items.is_empty() {
            return self.snapshot_mgr.end_tx(xid);
        }
        let (storage, log, prepared) = {
            let prepared = self
                .storage
                .prepare_worker_kv_commit(snapshot, xid, items, batch)?;
            (self.storage.clone(), self.log.clone(), prepared)
        };
        if let Some(log) = log {
            new_xl_batch_writer(log.clone()).append_sync(prepared.batch())?;
            log.flush()?;
        }
        storage.apply_prepared_commit(prepared)?;
        self.snapshot_mgr.end_tx(xid)
    }

    pub async fn worker_commit_put_batch_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: std::collections::BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<()> {
        if items.is_empty() {
            return self.snapshot_mgr.end_tx(xid);
        }
        let (storage, log, prepared) = {
            let prepared = self
                .storage
                .prepare_worker_kv_commit(snapshot, xid, items, batch)?;
            (self.storage.clone(), self.log.clone(), prepared)
        };
        if let Some(log) = log {
            new_xl_batch_writer(log.clone())
                .append(prepared.batch())
                .await?;
            log.flush_async().await?;
        }
        storage.apply_prepared_commit_async(prepared).await?;
        self.snapshot_mgr.end_tx(xid)
    }

    pub fn worker_commit_tx(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        let xid = tx.xid();
        if tx.is_empty() {
            return self.worker_rollback_tx(tx);
        }
        tx.build_write_ops();
        let (storage, log, prepared) = {
            let write_ops = tx.write_ops();
            let can_commit = self.tx_lock.try_lock_some(xid as OID, &write_ops);
            if !can_commit {
                return Err(m_error!(
                    EC::TxErr,
                    format!("transaction {} failed to acquire commit locks", xid)
                ));
            }
            let prepared = self.storage.prepare_commit(tx.as_ref())?;
            (self.storage.clone(), self.log.clone(), prepared)
        };
        let result = (|| -> RS<()> {
            if let Some(log) = log {
                new_xl_batch_writer(log.clone()).append_sync(prepared.batch())?;
                log.flush()?;
            }
            storage.apply_prepared_commit(prepared)?;
            Ok(())
        })();
        let write_ops = tx.write_ops();
        self.tx_lock.release(xid as OID, &write_ops);
        self.worker_rollback_tx(tx)?;
        result
    }

    pub async fn worker_commit_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        mudu_utils::scoped_task_trace!();

        let xid = tx.xid();

        trace!("worker_commit_tx_async {}", xid);
        if tx.is_empty() {
            return self.worker_rollback_tx(tx);
        }
        tx.build_write_ops();
        let (storage, log, prepared) = {
            let write_ops = tx.write_ops();
            let can_commit = self.tx_lock.try_lock_some(xid as OID, &write_ops);
            if !can_commit {
                return Err(m_error!(
                    EC::TxErr,
                    format!("transaction {} failed to acquire commit locks", xid)
                ));
            }
            let prepared = self.storage.prepare_commit_async(tx.as_ref()).await?;
            (self.storage.clone(), self.log.clone(), prepared)
        };
        trace!("log flush {}", xid);
        let result = async {
            if let Some(log) = log {
                task_trace!().watch("procedure.worker_execute.stage", "wal_append_start");
                new_xl_batch_writer(log.clone())
                    .append(prepared.batch())
                    .await?;
                task_trace!().watch("procedure.worker_execute.stage", "wal_append_done");
                task_trace!().watch("procedure.worker_execute.stage", "wal_flush_start");
                log.flush_async().await?;
                task_trace!().watch("procedure.worker_execute.stage", "wal_flush_done");
            }
            task_trace!().watch("procedure.worker_execute.stage", "storage_apply_start");
            storage.apply_prepared_commit_async(prepared).await?;
            task_trace!().watch("procedure.worker_execute.stage", "storage_apply_done");
            Ok(())
        }
        .await;
        trace!("log flush done {}", xid);
        let write_ops = tx.write_ops();
        self.tx_lock.release(xid as OID, &write_ops);
        self.worker_rollback_tx(tx)?;
        trace!("worker_commit_tx_async finish {}", xid);
        result
    }

    pub fn replay_worker_log_batch(&self, batch: XLBatch) -> RS<()> {
        let max_xid = batch.entries.iter().map(|entry| entry.xid).max();
        if let Some(max_xid) = max_xid {
            self.snapshot_mgr.observe_committed_ts(max_xid);
        }
        self.storage.replay_batch(batch)
    }

    pub fn ensure_partition_rpc_handler(self: &Arc<Self>) -> RS<()> {
        if self.partition_rpc_registered.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        debug!(
            worker_id = self.worker_id,
            "registering partition rpc handler"
        );
        let bus = current_message_bus()?;
        let contract = self.clone();
        bus.on_recv_callback(
            RecvFilter {
                dst: Some(EndpointId::Worker(self.worker_id)),
                kind: Some(PARTITION_RPC_REQUEST_KIND),
                ..RecvFilter::default()
            },
            Arc::new(move |envelope| {
                let contract = contract.clone();
                Box::pin(async move { contract.handle_partition_rpc(envelope).await })
            }),
        )?;
        Ok(())
    }
}

fn bootstrap_storage(storage: &Arc<WorkerStorage>) -> RS<()> {
    if storage.uses_async_runtime() {
        let storage = storage.clone();
        mudu_sys::task_async::block_on_tokio_current_thread(async move {
            storage.bootstrap_existing_tables_async().await
        })
        .map_err(|e| {
            m_error!(
                EC::TokioErr,
                "create tokio runtime for storage bootstrap failed",
                e
            )
        })?
        .map_err(|e| {
            m_error!(
                EC::StorageErr,
                "bootstrap worker storage from meta failed",
                e
            )
        })
    } else {
        storage.bootstrap_existing_tables_sync().map_err(|e| {
            m_error!(
                EC::StorageErr,
                "bootstrap worker storage from meta failed",
                e
            )
        })
    }
}

fn default_worker_storage_data_dir() -> String {
    std::env::temp_dir()
        .join(format!(
            "mududb-worker-storage-{}",
            mudu::common::id::gen_oid()
        ))
        .to_string_lossy()
        .to_string()
}

impl WorkerXContract {
    async fn handle_partition_rpc(&self, envelope: Envelope) -> RS<()> {
        debug!(
            worker_id = self.worker_id,
            src = ?envelope.src(),
            msg_id = envelope.msg_id(),
            "received partition rpc request"
        );
        let request = rmp_serde::from_slice::<PartitionRpcRequest>(envelope.payload())
            .map_err(|e| m_error!(EC::DecodeErr, "decode partition rpc request error", e))?;
        let response = match self.execute_partition_rpc(request).await {
            Ok(response) => response,
            Err(err) => PartitionRpcResponse::Err(err.to_string()),
        };
        let payload = rmp_serde::to_vec(&response)
            .map_err(|e| m_error!(EC::EncodeErr, "encode partition rpc response error", e))?;
        let bus = current_message_bus()?;
        bus.send(
            envelope.src().clone(),
            OutgoingMessage::new(PARTITION_RPC_RESPONSE_KIND, payload)
                .with_correlation_id(envelope.msg_id())
                .with_delivery(DeliveryMode::Response),
        )
        .await?;
        debug!(
            worker_id = self.worker_id,
            dst = ?envelope.src(),
            correlation_id = envelope.msg_id(),
            "sent partition rpc response"
        );
        Ok(())
    }

    async fn execute_partition_rpc(
        &self,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        match request {
            PartitionRpcRequest::ReadKey {
                table_id,
                partition_id,
                key,
                select,
            } => {
                debug!(
                    worker_id = self.worker_id,
                    table_id,
                    partition_id,
                    key_len = key.len(),
                    select_len = select.len(),
                    "execute partition rpc read_key"
                );
                let desc = self.meta_mgr.get_table_by_id(table_id).await?;
                let tx_mgr = self.worker_begin_tx()?;
                let opt_value = self
                    .storage
                    .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
                    .await?;
                self.worker_rollback_tx(tx_mgr)?;
                let projected = opt_value
                    .map(|value| {
                        project_selected_fields(&desc, &key, &value, &VecSelTerm::new(select))
                    })
                    .transpose()?;
                Ok(PartitionRpcResponse::ReadKey(projected))
            }
            PartitionRpcRequest::ReadRange {
                table_id,
                partition_id,
                start,
                end,
                select,
            } => {
                debug!(
                    worker_id = self.worker_id,
                    table_id,
                    partition_id,
                    select_len = select.len(),
                    start = ?start,
                    end = ?end,
                    "execute partition rpc read_range"
                );
                let desc = self.meta_mgr.get_table_by_id(table_id).await?;
                let tx_mgr = self.worker_begin_tx()?;
                let rows = self
                    .storage
                    .range_on_partition(
                        table_id,
                        Some(partition_id),
                        (rpc_bound_as_ref(&start), rpc_bound_as_ref(&end)),
                        tx_mgr.as_ref(),
                    )
                    .await?;
                self.worker_rollback_tx(tx_mgr)?;
                let mut projected = Vec::with_capacity(rows.len());
                for (key, value) in rows {
                    projected.push(project_selected_fields(
                        &desc,
                        &key,
                        &value,
                        &VecSelTerm::new(select.clone()),
                    )?);
                }
                Ok(PartitionRpcResponse::ReadRange(projected))
            }
            PartitionRpcRequest::Insert {
                table_id,
                partition_id,
                key,
                value,
            } => {
                debug!(
                    worker_id = self.worker_id,
                    table_id,
                    partition_id,
                    key_len = key.len(),
                    value_len = value.len(),
                    "execute partition rpc insert"
                );
                let tx_mgr = self.worker_begin_tx()?;
                let current = self
                    .storage
                    .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
                    .await?;
                if current.is_some() {
                    self.worker_rollback_tx(tx_mgr)?;
                    return Err(m_error!(EC::ExistingSuchElement, "existing key"));
                }
                self.storage
                    .put_on_partition(table_id, Some(partition_id), key, value, tx_mgr.as_ref())
                    .await?;
                self.worker_commit_tx_async(tx_mgr).await?;
                Ok(PartitionRpcResponse::Insert)
            }
            PartitionRpcRequest::Delete {
                table_id,
                partition_id,
                key,
            } => {
                debug!(
                    worker_id = self.worker_id,
                    table_id,
                    partition_id,
                    key_len = key.len(),
                    "execute partition rpc delete"
                );
                let tx_mgr = self.worker_begin_tx()?;
                let deleted = self
                    .storage
                    .remove_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
                    .await?;
                self.worker_commit_tx_async(tx_mgr).await?;
                Ok(PartitionRpcResponse::Delete(usize::from(deleted.is_some())))
            }
            PartitionRpcRequest::Update {
                table_id,
                partition_id,
                key,
                values,
            } => {
                debug!(
                    worker_id = self.worker_id,
                    table_id,
                    partition_id,
                    key_len = key.len(),
                    value_pairs = values.len(),
                    "execute partition rpc update"
                );
                let desc = self.meta_mgr.get_table_by_id(table_id).await?;
                let tx_mgr = self.worker_begin_tx()?;
                let current = self
                    .storage
                    .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
                    .await?;
                let Some(current) = current else {
                    self.worker_rollback_tx(tx_mgr)?;
                    return Ok(PartitionRpcResponse::Update(0));
                };
                let updated = apply_value_update(&current, &VecDatum::new(values), &desc)?;
                self.storage
                    .put_on_partition(table_id, Some(partition_id), key, updated, tx_mgr.as_ref())
                    .await?;
                self.worker_commit_tx_async(tx_mgr).await?;
                Ok(PartitionRpcResponse::Update(1))
            }
        }
    }

    async fn send_partition_rpc(
        &self,
        target_worker_id: OID,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            target_worker_id,
            request = ?request,
            "sending partition rpc request"
        );
        let bus = current_message_bus()?;
        let payload = rmp_serde::to_vec(&request)
            .map_err(|e| m_error!(EC::EncodeErr, "encode partition rpc request error", e))?;
        let msg_id = bus
            .send(
                EndpointId::Worker(target_worker_id),
                OutgoingMessage::new(PARTITION_RPC_REQUEST_KIND, payload)
                    .with_delivery(DeliveryMode::Request),
            )
            .await?;
        debug!(
            worker_id = self.worker_id,
            target_worker_id, msg_id, "waiting partition rpc response"
        );
        let envelope = mudu_sys::task_async::timeout(
            Duration::from_secs(10),
            bus.recv(RecvFilter {
                src: Some(EndpointId::Worker(target_worker_id)),
                dst: Some(EndpointId::Worker(self.worker_id)),
                kind: Some(PARTITION_RPC_RESPONSE_KIND),
                correlation_id: Some(msg_id),
            }),
        )
        .await
        .ok_or_else(|| {
            m_error!(
                EC::TokioErr,
                format!(
                    "partition rpc response timeout: server={}, worker={}, target_worker={}, msg_id={}",
                    self.server_instance_id, self.worker_id, target_worker_id, msg_id
                )
            )
        })??;
        debug!(
            worker_id = self.worker_id,
            target_worker_id,
            msg_id,
            received_msg_id = envelope.msg_id(),
            received_correlation_id = ?envelope.correlation_id(),
            "received partition rpc response envelope"
        );
        rmp_serde::from_slice(envelope.payload())
            .map_err(|e| m_error!(EC::DecodeErr, "decode partition rpc response error", e))
    }

    async fn remote_read_key(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<DatBin>>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::ReadKey {
                    table_id,
                    partition_id,
                    key,
                    select,
                },
            )
            .await?
        {
            PartitionRpcResponse::ReadKey(value) => Ok(value),
            PartitionRpcResponse::Err(err) => Err(m_error!(EC::InternalErr, err)),
            _ => Err(m_error!(
                EC::InternalErr,
                "unexpected read_key rpc response"
            )),
        }
    }

    async fn remote_read_range(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        start: RpcBound,
        end: RpcBound,
        select: Vec<AttrIndex>,
    ) -> RS<Vec<Vec<DatBin>>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::ReadRange {
                    table_id,
                    partition_id,
                    start,
                    end,
                    select,
                },
            )
            .await?
        {
            PartitionRpcResponse::ReadRange(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(m_error!(EC::InternalErr, err)),
            _ => Err(m_error!(
                EC::InternalErr,
                "unexpected read_range rpc response"
            )),
        }
    }

    async fn remote_insert(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::Insert {
                    table_id,
                    partition_id,
                    key,
                    value,
                },
            )
            .await?
        {
            PartitionRpcResponse::Insert => Ok(()),
            PartitionRpcResponse::Err(err) => Err(m_error!(EC::InternalErr, err)),
            _ => Err(m_error!(EC::InternalErr, "unexpected insert rpc response")),
        }
    }

    async fn remote_delete(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
    ) -> RS<usize> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::Delete {
                    table_id,
                    partition_id,
                    key,
                },
            )
            .await?
        {
            PartitionRpcResponse::Delete(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(m_error!(EC::InternalErr, err)),
            _ => Err(m_error!(EC::InternalErr, "unexpected delete rpc response")),
        }
    }

    async fn remote_update(
        &self,
        target_worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        values: Vec<(AttrIndex, Vec<u8>)>,
    ) -> RS<usize> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::Update {
                    table_id,
                    partition_id,
                    key,
                    values,
                },
            )
            .await?
        {
            PartitionRpcResponse::Update(rows) => Ok(rows),
            PartitionRpcResponse::Err(err) => Err(m_error!(EC::InternalErr, err)),
            _ => Err(m_error!(EC::InternalErr, "unexpected update rpc response")),
        }
    }

    fn _begin_tx(&self) -> Arc<dyn TxMgr> {
        Arc::new(WorkerTxManager::new(self.snapshot_mgr.begin_tx()))
    }

    async fn _insert(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            key_cols = keys.data().len(),
            value_cols = values.data().len(),
            "insert begin"
        );
        let key = build_key_tuple(keys, &desc)?;
        let value = build_value_tuple(values, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), keys)
            .await?;
        debug!(
            worker_id = self.worker_id,
            table_id,
            target_partition = ?target_partition,
            "insert routed partition"
        );
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    debug!(
                        worker_id = self.worker_id,
                        table_id,
                        partition_id,
                        target_worker_id = worker_id,
                        "insert forwarding to remote worker"
                    );
                    return self
                        .remote_insert(worker_id, table_id, partition_id, key, value)
                        .await;
                }
                _ => {}
            }
        }
        debug!(
            worker_id = self.worker_id,
            table_id,
            target_partition = ?target_partition,
            "insert checking existing key locally"
        );
        let contain_key = self
            .storage
            .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?;
        if contain_key.is_some() {
            Err(m_error!(EC::ExistingSuchElement, "existing key"))
        } else {
            debug!(
                worker_id = self.worker_id,
                table_id,
                target_partition = ?target_partition,
                "insert writing key locally"
            );
            self.storage
                .put_on_partition(table_id, target_partition, key, value, tx_mgr.as_ref())
                .await
        }
    }

    async fn _read_key(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<DatBin>>> {
        let key = build_key_tuple(pred_key, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), pred_key)
            .await?;
        let opt_value = match target_partition {
            Some(partition_id) => match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    self.remote_read_key(
                        worker_id,
                        table_id,
                        partition_id,
                        key.clone(),
                        select.vec().to_vec(),
                    )
                    .await?
                }
                _ => {
                    let result = self
                        .storage
                        .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
                        .await?;
                    result
                        .map(|value| project_selected_fields(&desc, &key, &value, select))
                        .transpose()?
                }
            },
            None => {
                let result = self
                    .storage
                    .get_on_partition(table_id, None, &key, tx_mgr.as_ref())
                    .await?;
                result
                    .map(|value| project_selected_fields(&desc, &key, &value, select))
                    .transpose()?
            }
        };
        match opt_value {
            Some(value) => Ok(Some(value)),
            None => Ok(None),
        }
    }

    async fn _read_range(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        ensure_supported_predicate(pred_non_key)?;
        let start = build_bound_key(pred_key.start(), &desc)?;
        let end = build_bound_key(pred_key.end(), &desc)?;
        let target_partitions = self
            .partition_router
            .route_range_partitions(table_id, desc.as_ref(), pred_key.start(), pred_key.end())
            .await?;
        let mut projected = Vec::new();
        match target_partitions {
            Some(partitions) => {
                for partition_id in partitions {
                    match self.resolve_partition_worker(partition_id).await? {
                        Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                            if matches!(pred_non_key, Predicate::KeyPrefixEq(_)) {
                                return Err(m_error!(
                                    EC::NotImplemented,
                                    "key-prefix range filtering is not implemented for remote partitions"
                                ));
                            }
                            let rows = self
                                .remote_read_range(
                                    worker_id,
                                    table_id,
                                    partition_id,
                                    rpc_bound_from_key_bound(pred_key.start(), &desc)?,
                                    rpc_bound_from_key_bound(pred_key.end(), &desc)?,
                                    select.vec().to_vec(),
                                )
                                .await?;
                            for row in rows {
                                projected.push(TupleRow::new(row));
                            }
                        }
                        _ => {
                            let rows = self
                                .storage
                                .range_on_partition(
                                    table_id,
                                    Some(partition_id),
                                    (start.clone(), end.clone()),
                                    tx_mgr.as_ref(),
                                )
                                .await?;
                            for (key, value) in rows {
                                if !matches_predicate(&desc, &key, &value, pred_non_key)? {
                                    continue;
                                }
                                projected.push(TupleRow::new(project_selected_fields(
                                    &desc, &key, &value, select,
                                )?));
                            }
                        }
                    }
                }
            }
            None => {
                let rows = self
                    .storage
                    .range(table_id, (start, end), tx_mgr.as_ref())
                    .await?;
                for (key, value) in rows {
                    if !matches_predicate(&desc, &key, &value, pred_non_key)? {
                        continue;
                    }
                    projected.push(TupleRow::new(project_selected_fields(
                        &desc, &key, &value, select,
                    )?));
                }
            }
        }
        Ok(Arc::new(VecCursor {
            inner: Mutex::new(VecCursorInner {
                rows: projected,
                index: 0,
            }),
        }))
    }

    async fn _delete(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        ensure_supported_predicate(pred_non_key)?;
        let key = build_key_tuple(pred_key, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), pred_key)
            .await?;
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    return self
                        .remote_delete(worker_id, table_id, partition_id, key)
                        .await;
                }
                _ => {}
            }
        }
        let deleted = self
            .storage
            .remove_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?;
        Ok(usize::from(deleted.is_some()))
    }

    async fn _update(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        ensure_supported_predicate(pred_non_key)?;
        let key = build_key_tuple(pred_key, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), pred_key)
            .await?;
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    return self
                        .remote_update(
                            worker_id,
                            table_id,
                            partition_id,
                            key,
                            values.data().clone(),
                        )
                        .await;
                }
                _ => {}
            }
        }
        let current = self
            .storage
            .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?;
        let Some(current) = current else {
            return Ok(0);
        };
        let updated = apply_value_update(&current, values, &desc)?;
        self.storage
            .put_on_partition(table_id, target_partition, key, updated, tx_mgr.as_ref())
            .await
            .map(|()| 1)
    }
}

#[async_trait]
impl XContract for WorkerXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, schema: &SchemaTable) -> RS<()> {
        self.storage.create_table_async(schema).await
    }

    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, oid: OID) -> RS<()> {
        self.storage.drop_table_async(oid).await
    }

    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &AlterTable,
    ) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "alter table is not implemented"
        ))
    }

    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(self._begin_tx())
    }

    async fn commit_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        self.worker_commit_tx_async(tx_mgr).await
    }

    async fn abort_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        self.worker_rollback_tx(tx_mgr)
    }

    async fn update(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        opt_update: &OptUpdate,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._update(
            desc,
            tx_mgr,
            table_id,
            pred_key,
            pred_non_key,
            values,
            opt_update,
        )
        .await
    }

    async fn read_key(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Option<Vec<DatBin>>> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._read_key(desc, tx_mgr, table_id, pred_key, select, opt_read)
            .await
    }

    async fn read_range(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._read_range(
            desc,
            tx_mgr,
            table_id,
            pred_key,
            pred_non_key,
            select,
            opt_read,
        )
        .await
    }

    async fn delete(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        opt_delete: &OptDelete,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._delete(desc, tx_mgr, table_id, pred_key, pred_non_key, opt_delete)
            .await
    }

    async fn insert(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        opt_insert: &OptInsert,
    ) -> RS<()> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._insert(desc, tx_mgr, table_id, keys, values, opt_insert)
            .await
    }
}

impl WorkerXContract {
    pub fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.meta_mgr.clone()
    }
}

#[async_trait]
impl RSCursor for VecCursor {
    async fn next(&self) -> RS<Option<TupleRow>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "range cursor lock poisoned"))?;
        if inner.index >= inner.rows.len() {
            return Ok(None);
        }
        let row = inner.rows[inner.index].clone();
        inner.index += 1;
        Ok(Some(row))
    }
}

fn ensure_supported_predicate(predicate: &Predicate) -> RS<()> {
    match predicate {
        Predicate::CNF(items) | Predicate::DNF(items) if items.is_empty() => Ok(()),
        Predicate::KeyPrefixEq(_) => Ok(()),
        Predicate::CNF(items) | Predicate::DNF(items) => {
            let _ = items
                .iter()
                .flatten()
                .map(|(_oid, _filter): &(AttrIndex, Filter)| ())
                .count();
            Err(m_error!(
                EC::NotImplemented,
                "non-key predicates are not implemented in io_uring xcontract"
            ))
        }
    }
}

fn matches_predicate(
    desc: &TableDesc,
    key: &[u8],
    _value: &[u8],
    predicate: &Predicate,
) -> RS<bool> {
    match predicate {
        Predicate::CNF(items) | Predicate::DNF(items) if items.is_empty() => Ok(true),
        Predicate::KeyPrefixEq(prefix) => {
            for (attr, expected) in prefix {
                let field = desc.get_attr(*attr);
                let Some(primary_index) = field.primary_index() else {
                    return Ok(false);
                };
                let field_desc = desc.key_desc().get_field_desc(primary_index);
                let actual = field_desc.get(key)?;
                if actual != expected.as_slice() {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Predicate::CNF(_) | Predicate::DNF(_) => Err(m_error!(
            EC::NotImplemented,
            "non-key predicates are not implemented in io_uring xcontract"
        )),
    }
}

fn build_key_tuple(data: &VecDatum, desc: &TableDesc) -> RS<Vec<u8>> {
    build_tuple_for::<true>(data.data(), desc)
}

fn build_value_tuple(data: &VecDatum, desc: &TableDesc) -> RS<Vec<u8>> {
    build_tuple_for::<false>(data.data(), desc)
}

fn build_tuple_for<const IS_KEY: bool>(
    data: &Vec<(AttrIndex, DatBin)>,
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    let mut vec_data = data.clone();
    let mut ok = true;
    vec_data.sort_by(|(id1, _), (id2, _)| {
        let (f1, f2) = (desc.get_attr(*id1), desc.get_attr(*id2));
        if f1.primary_index().is_some() != IS_KEY || f2.primary_index().is_some() != IS_KEY {
            ok = false;
        }
        f1.datum_index().cmp(&f2.datum_index())
    });
    if !ok {
        return Err(m_error!(EC::TupleErr));
    }
    let tuple_desc = if IS_KEY {
        desc.key_desc()
    } else {
        desc.value_desc()
    };
    let values: Vec<_> = vec_data.into_iter().map(|(_, v)| v).collect();
    if IS_KEY && tuple_desc.field_count() != values.len() {
        let expected_key_fields = desc
            .key_indices()
            .iter()
            .map(|index| desc.get_attr(*index).name().clone())
            .collect::<Vec<_>>();
        let provided_fields = data
            .iter()
            .map(|(attr, _)| {
                let field = desc.get_attr(*attr);
                format!(
                    "{}(column_index={}, datum_index={}, primary_index={:?})",
                    field.name(),
                    field.column_index(),
                    field.datum_index(),
                    field.primary_index()
                )
            })
            .collect::<Vec<_>>();
        return Err(m_error!(
            EC::TupleErr,
            format!(
                "build key tuple width mismatch for table {}: expected {} key fields {:?}, got {} provided fields {:?}",
                desc.name(),
                tuple_desc.field_count(),
                expected_key_fields,
                values.len(),
                provided_fields,
            )
        ));
    }
    if IS_KEY {
        return build_tuple(&values, tuple_desc);
    }

    let value_len = tuple_desc.field_count();
    let mut completed = vec![None; value_len];
    for (attr, value) in data {
        let field = desc.get_attr(*attr);
        if field.primary_index().is_some() {
            return Err(m_error!(EC::TupleErr));
        }
        let datum_index = field.datum_index() as usize;
        if datum_index >= value_len || completed[datum_index].is_some() {
            return Err(m_error!(EC::TupleErr));
        }
        completed[datum_index] = Some(value.clone());
    }
    for attr in desc.value_indices() {
        let field = desc.get_attr(*attr);
        let datum_index = field.datum_index() as usize;
        if completed[datum_index].is_some() {
            continue;
        }
        let default = field.type_desc().dat_type_id().fn_default()(field.type_desc())
            .map_err(|e| e.to_m_err())?;
        completed[datum_index] =
            Some(send_binary(&default, field.type_desc()).map_err(|e| e.to_m_err())?);
    }
    let completed = completed
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            m_error!(EC::TupleErr)
        })?;
    build_tuple(&completed, tuple_desc)
}

fn build_bound_key(
    bound: &Bound<Vec<(AttrIndex, DatBin)>>,
    desc: &TableDesc,
) -> RS<Bound<&'static [u8]>> {
    match bound {
        Bound::Included(values) => {
            let tuple = build_key_tuple(&VecDatum::new(values.clone()), desc)?;
            Ok(Bound::Included(Box::leak(tuple.into_boxed_slice())))
        }
        Bound::Excluded(values) => {
            let tuple = build_key_tuple(&VecDatum::new(values.clone()), desc)?;
            Ok(Bound::Excluded(Box::leak(tuple.into_boxed_slice())))
        }
        Bound::Unbounded => Ok(Bound::Unbounded),
    }
}

fn rpc_bound_from_key_bound(
    bound: &Bound<Vec<(AttrIndex, DatBin)>>,
    desc: &TableDesc,
) -> RS<RpcBound> {
    match bound {
        Bound::Included(values) => Ok(RpcBound::Included(build_key_tuple(
            &VecDatum::new(values.clone()),
            desc,
        )?)),
        Bound::Excluded(values) => Ok(RpcBound::Excluded(build_key_tuple(
            &VecDatum::new(values.clone()),
            desc,
        )?)),
        Bound::Unbounded => Ok(RpcBound::Unbounded),
    }
}

fn rpc_bound_as_ref(bound: &RpcBound) -> Bound<&[u8]> {
    match bound {
        RpcBound::Included(bytes) => Bound::Included(bytes.as_slice()),
        RpcBound::Excluded(bytes) => Bound::Excluded(bytes.as_slice()),
        RpcBound::Unbounded => Bound::Unbounded,
    }
}

fn project_selected_fields(
    desc: &TableDesc,
    key: &[u8],
    value: &[u8],
    select: &VecSelTerm,
) -> RS<Vec<DatBin>> {
    let mut tuple_ret = vec![];
    for i in select.vec() {
        let f = desc.get_attr(*i);
        let index = f.datum_index();
        let field_desc = if f.primary_index().is_some() {
            desc.key_desc().get_field_desc(index)
        } else {
            desc.value_desc().get_field_desc(index)
        };
        let src = if f.primary_index().is_some() {
            key
        } else {
            value
        };
        let slice = field_desc.get(src)?;
        tuple_ret.push(slice.to_vec());
    }
    Ok(tuple_ret)
}

fn apply_value_update(current: &TupleRaw, values: &VecDatum, desc: &TableDesc) -> RS<Vec<u8>> {
    let mut updated = current.clone();
    let mut data = values.data().clone();
    data.sort_by_key(|(attr, _)| desc.get_attr(*attr).datum_index());
    for (id, dat) in data.iter() {
        let field = desc.get_attr(*id);
        let mut delta = vec![];
        update_tuple(
            field.datum_index() as usize,
            dat,
            desc.value_desc(),
            current,
            &mut delta,
        )?;
        for item in delta {
            item.apply_to(&mut updated);
        }
    }
    Ok(updated)
}

fn single_put_batch(xid: u64, key: Vec<u8>, value: Vec<u8>) -> XLBatch {
    XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
        xid,
        ops: vec![
            crate::wal::xl_entry::TxOp::Begin,
            crate::wal::xl_entry::TxOp::Insert(crate::wal::xl_data_op::XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key,
                value,
            }),
            crate::wal::xl_entry::TxOp::Commit,
        ],
    }])
}

fn single_delete_batch(xid: u64, key: Vec<u8>) -> XLBatch {
    XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
        xid,
        ops: vec![
            crate::wal::xl_entry::TxOp::Begin,
            crate::wal::xl_entry::TxOp::Delete(crate::wal::xl_data_op::XLDelete {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key,
            }),
            crate::wal::xl_entry::TxOp::Commit,
        ],
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::table_info::TableInfo;
    use crate::server::test_meta_mgr::TestMetaMgr;
    use crate::wal::worker_log::{decode_frames, ChunkedWorkerLogBackend, WorkerLogLayout};
    use crate::wal::xl_data_op::XLInsert;
    use crate::wal::xl_entry::TxOp;
    use mudu::common::id::gen_oid;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dt_fn_param::DatType;
    use mudu_type::dt_info::DTInfo;
    use std::env::temp_dir;
    use std::future::Future;

    fn block_on<F>(fut: F) -> F::Output
    where
        F: Future,
    {
        mudu_sys::task_async::build_current_thread_runtime()
            .unwrap()
            .block_on(fut)
    }

    fn test_schema() -> SchemaTable {
        SchemaTable::new(
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_text(DatTypeID::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_text(DatTypeID::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        )
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

    fn datum_string(v: &str) -> Vec<u8> {
        mudu_type::dt_function::send_binary(
            &mudu_type::dat_value::DatValue::from_string(v.to_string()),
            &mudu_type::dat_type::DatType::default_for(mudu_type::dat_type_id::DatTypeID::String),
        )
        .unwrap()
    }

    fn wallet_users_schema() -> SchemaTable {
        use crate::contract::schema_column::SchemaColumn;
        use mudu_type::dt_info::DTInfo;

        SchemaTable::new(
            "users".to_string(),
            vec![
                SchemaColumn::new(
                    "user_id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
                ),
                SchemaColumn::new(
                    "phone".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
                ),
                SchemaColumn::new(
                    "email".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
                ),
                SchemaColumn::new(
                    "password".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
                ),
                SchemaColumn::new(
                    "created_at".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "updated_at".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
            ],
            vec![0],
            vec![1, 2, 3, 4, 5, 6],
        )
    }

    #[test]
    fn relation_commit_log_round_trips() {
        block_on(async move {
            let r = _relation_commit_log_round_trips().await;
            assert!(r.is_ok())
        })
    }

    async fn _relation_commit_log_round_trips() -> RS<()> {
        let mgr = Arc::new(TestMetaMgr::new());
        let storage = WorkerStorage::new(
            mgr.clone(),
            0,
            std::env::temp_dir()
                .join(format!(
                    "xcontract_relation_log_{}",
                    mudu::common::id::gen_oid()
                ))
                .to_string_lossy()
                .to_string(),
        );
        let schema = test_schema();
        let table_id = schema.id();
        storage.create_table_async(&schema).await?;
        let mut txm = WorkerTxManager::new(crate::server::worker_snapshot::WorkerSnapshot::new(
            9,
            vec![],
        ));
        storage
            .put(table_id, b"k1".to_vec(), b"v1".to_vec(), &mut txm)
            .await?;
        storage.remove(table_id, b"k1", &mut txm).await?;
        let prepared = storage.prepare_commit(&txm).unwrap();

        assert_eq!(prepared.batch().entries.len(), 1);
        assert_eq!(prepared.batch().entries[0].xid, 9);
        assert!(matches!(prepared.batch().entries[0].ops[0], TxOp::Begin));
        Ok(())
    }

    #[test]
    fn iouring_xcontract_commit_persists_relation_log() {
        block_on(async move {
            let r = _iouring_xcontract_commit_persists_relation_log().await;
            assert!(r.is_ok())
        })
    }

    async fn _iouring_xcontract_commit_persists_relation_log() -> RS<()> {
        let dir = temp_dir().join(format!("iouring_xcontract_log_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
        let log = ChunkedWorkerLogBackend::new(layout.clone()).unwrap();
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, Some(log)).unwrap();

        let ddl_tx = contract.begin_tx().await?;
        contract.create_table(ddl_tx.clone(), &schema).await?;
        contract.commit_tx(ddl_tx).await?;
        let tx_mgr = contract.begin_tx().await?;
        let keys = key_row(1);
        let values = value_row(10);
        let opt_insert = OptInsert::default();
        contract
            .insert(
            tx_mgr.clone(),
            table_id,
            &keys,
            &values,
            &opt_insert,
        )
            .await?;
        contract.commit_tx(tx_mgr).await?;

        let bytes = std::fs::read(layout.chunk_path(0)).unwrap();
        let frames = decode_frames(&bytes).unwrap();
        let decoded = crate::wal::xl_batch::decode_xl_batches(&frames).unwrap();
        assert_eq!(decoded.len(), 1);
        let insert = decoded[0].entries[0]
            .ops
            .iter()
            .find_map(|op| match op {
                TxOp::Insert(insert) => Some(insert),
                _ => None,
            })
            .unwrap();
        assert_eq!(insert.table_id, table_id);
        assert_eq!(
            insert.key,
            build_key_tuple(&key_row(1), &meta_table(&schema).unwrap()).unwrap()
        );
        assert_eq!(
            insert.value,
            build_value_tuple(&value_row(10), &meta_table(&schema).unwrap()).unwrap()
        );
        Ok(())
    }

    #[test]
    fn iouring_xcontract_replay_restores_worker_kv_and_relation_rows() {
        block_on(async move {
            let r = _iouring_xcontract_replay_restores_worker_kv_and_relation_rows().await;
            assert!(r.is_ok())
        })
    }

    async fn _iouring_xcontract_replay_restores_worker_kv_and_relation_rows() -> RS<()> {
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

        let tx_mgr = contract.begin_tx().await?;
        contract.create_table(tx_mgr.clone(), &schema).await?;
        contract.commit_tx(tx_mgr).await?;
        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 11,
            ops: vec![
                TxOp::Begin,
                TxOp::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"wk".to_vec(),
                    value: b"wv".to_vec(),
                }),
                TxOp::Insert(XLInsert {
                    table_id,
                    partition_id: 0,
                    tuple_id: 0,
                    key: build_key_tuple(&key_row(3), &meta_table(&schema).unwrap()).unwrap(),
                    value: build_value_tuple(&value_row(30), &meta_table(&schema).unwrap())
                        .unwrap(),
                }),
                TxOp::Commit,
            ],
        }]);

        contract.replay_worker_log_batch(batch).unwrap();

        assert_eq!(contract.worker_get(b"wk").unwrap(), Some(b"wv".to_vec()));

        let xid = contract.begin_tx().await?;
        let pred_key = key_row(3);
        let select = VecSelTerm::new(vec![1]);
        let opt_read = OptRead::default();
        let relation = contract
            .read_key(
            xid,
            table_id,
            &pred_key,
            &select,
            &opt_read,
        )
            .await?;
        assert_eq!(relation, Some(vec![datum(30)]));
        Ok(())
    }

    #[test]
    fn build_value_tuple_supports_partial_insert_with_mixed_types() {
        let schema = wallet_users_schema();
        let desc = meta_table(&schema).unwrap();
        let input = VecDatum::new(vec![
            (1, datum_string("Alice")),
            (2, datum_string("12345678")),
            (3, datum_string("alice@xxx.com")),
            (4, datum_string("aaa")),
            (5, datum(0)),
        ]);
        let tuple = build_value_tuple(&input, &desc).unwrap();
        assert!(!tuple.is_empty());
    }

    #[test]
    fn iouring_xcontract_replay_applies_worker_kv_delete() {
        let contract = WorkerXContract::with_worker_log(
            ChunkedWorkerLogBackend::new(
                WorkerLogLayout::new(
                    temp_dir().join(format!("iouring_xcontract_worker_log_{}", gen_oid())),
                    gen_oid(),
                    4096,
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        contract.worker_put(b"wk".to_vec(), b"wv".to_vec()).unwrap();
        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 7,
            ops: vec![
                TxOp::Begin,
                TxOp::Delete(crate::wal::xl_data_op::XLDelete {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"wk".to_vec(),
                }),
                TxOp::Commit,
            ],
        }]);

        contract.replay_worker_log_batch(batch).unwrap();

        assert_eq!(contract.worker_get(b"wk").unwrap(), None);
    }

    #[test]
    fn iouring_xcontract_update_maps_table_attr_to_value_tuple_index() {
        block_on(async move {
            let r = _iouring_xcontract_update_maps_table_attr_to_value_tuple_index().await;
            assert!(r.is_ok())
        })
    }

    async fn _iouring_xcontract_update_maps_table_attr_to_value_tuple_index() -> RS<()> {
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

        let ddl_tx = contract.begin_tx().await?;
        contract.create_table(ddl_tx.clone(), &schema).await?;
        contract.commit_tx(ddl_tx).await?;

        let insert_tx = contract.begin_tx().await?;
        let insert_key = key_row(1);
        let insert_value = value_row(10);
        let opt_insert = OptInsert::default();
        contract
            .insert(
            insert_tx.clone(),
            table_id,
            &insert_key,
            &insert_value,
            &opt_insert,
        )
            .await?;
        contract.commit_tx(insert_tx).await?;

        let update_tx = contract.begin_tx().await?;
        let update_key = key_row(1);
        let pred_non_key = Predicate::CNF(vec![]);
        let update_value = value_row(20);
        let updated = contract
            .update(
            update_tx.clone(),
            table_id,
            &update_key,
            &pred_non_key,
            &update_value,
            &OptUpdate {},
        )
            .await?;
        assert_eq!(updated, 1);
        contract.commit_tx(update_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let read_key = key_row(1);
        let select = VecSelTerm::new(vec![1]);
        let opt_read = OptRead::default();
        let relation = contract
            .read_key(
            read_tx,
            table_id,
            &read_key,
            &select,
            &opt_read,
        )
            .await?;
        assert_eq!(relation, Some(vec![datum(20)]));
        Ok(())
    }

    fn meta_table(schema: &SchemaTable) -> RS<Arc<TableDesc>> {
        TableInfo::new(schema.clone())?.table_desc()
    }
}
