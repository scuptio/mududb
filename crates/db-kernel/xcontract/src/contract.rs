//! The per-worker contract type and its commit-routing state.
//!
//! [`WorkerXContract`] is the aggregate that holds the worker's components
//! (storage, queue registries, lock manager, RPC dedup, replication wiring).
//! Its methods are kept thin: construction/initialization/accessors live here,
//! while the per-component business logic lives in the component modules.

use crate::consumer_registry::ConsumerRegistry;
use crate::queue_activation::ActivationRegistry;
use crate::queue_types::QueueRegistry;
use crate::queue_wait_registry::QueueWaitRegistry;
use crate::worker_storage::WorkerStorage;
use crate::x_lock_mgr::XLockMgr;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::worker_snapshot::WorkerSnapshotMgr;
use ::catalog::repl_ctrl::{RaftShareView, ReplAckWait, ReplControl};
use ::storage::prepared_commit::CommitSequencer;
use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogLayout};
use ::xengine::partition_router::PartitionRouter;
use mudu::channel::ServerInstanceId;
use mudu::common::id::OID;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::SMutex;
use mudu_utils::channel::ServiceHandle;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::sync::Arc;

/// Per-worker implementation of the `XContract` storage engine contract:
/// transactional relation DML and worker-local KV, WAL-backed commit, and
/// cross-partition routing over the worker channel.
pub struct WorkerXContract {
    pub(crate) server_instance_id: ServerInstanceId,
    pub(crate) worker_id: OID,
    pub(crate) default_unpartitioned_worker_id: OID,
    pub(crate) meta_mgr: Arc<dyn MetaMgr>,
    pub(crate) storage: Arc<WorkerStorage>,
    pub(crate) partition_router: PartitionRouter,
    pub(crate) partition_rpc_registered: AtomicBool,
    /// Receiver-side dedup cache of the partition RPC service.
    pub(crate) rpc_dedup: crate::rpc_dedup::RpcDedupCache,
    /// Keeps the partition RPC service registration on the worker channel
    /// alive; dropping it unregisters the handler.
    pub(crate) rpc_service: SMutex<Option<ServiceHandle>>,
    pub(crate) log: SMutex<Option<ChunkedWorkerLogBackend>>,
    pub(crate) log_layout: WorkerLogLayout,
    pub(crate) active_sessions: Arc<AtomicUsize>,
    /// Optional runtime I/O provider used by the io_uring worker loop. When
    /// present, WAL initialization scans the tail with the default (tokio)
    /// provider but the backend performs steady-state I/O via this provider.
    pub(crate) async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    pub(crate) snapshot_mgr: WorkerSnapshotMgr,
    pub(crate) tx_lock: XLockMgr,
    /// Raft state shared with this worker when the server runs a
    /// replication cluster; installed after construction by the server.
    pub(crate) raft_share: SMutex<Option<Arc<dyn RaftShareView>>>,
    /// The worker's commit sequencer (leader commit path), installed with
    /// `raft_share` when the worker's replication tasks start.
    pub(crate) commit_sequencer: SMutex<Option<Arc<dyn CommitSequencer>>>,
    /// Stream-mode control (write gate + sync commit path), installed
    /// when the server runs in stream mode.
    pub(crate) repl_ctl: SMutex<Option<Arc<dyn ReplControl>>>,
    /// The worker's StreamFollower ack waiter (stream-mode Primary with
    /// `sync = true`), installed when the worker's replication tasks start.
    pub(crate) repl_ack_waiter: SMutex<Option<Arc<dyn ReplAckWait>>>,
    /// Per-queue runtime counters (next sequence number and acked frontier)
    /// of the queues hosted on this worker.
    pub(crate) queue_registry: Arc<QueueRegistry>,
    /// Parked `RECEIVE ... WAIT` waiters of the queues hosted on this worker;
    /// the commit apply path wakes their queue heads.
    pub(crate) queue_wait_registry: QueueWaitRegistry,
    /// Named consumer cursors (fan-out mode) of the queues hosted on this
    /// worker, mirroring their `_queue_consumer` rows.
    pub(crate) consumer_registry: ConsumerRegistry,
    /// Registered queue activations (P1) of the queues hosted on this worker;
    /// the commit apply path notifies them. Volatile: in-memory only, lost on
    /// restart.
    pub(crate) activation_registry: ActivationRegistry,
    /// Cluster routing directory, present when the server runs with a static
    /// cluster topology (multi-node-group scale-out); `None` keeps the
    /// single-group behavior.
    pub(crate) cluster_router: Option<Arc<dyn ::catalog::contract::cluster_router::ClusterRouter>>,
    /// Serializes participant validation through quorum commit; retries cannot
    /// replace the write set of an in-flight prepare.
    pub(crate) participant_gate: mudu_sys::tokio::sync::Mutex<()>,
    pub(crate) tx_decision_client:
        SMutex<Option<Arc<dyn catalog::contract::tx_decision::TxDecisionClient>>>,
    /// Partition adoption hook (phase 3: partition moves), installed by the
    /// server: the target worker registers ownership of an adopted
    /// partition (registry marker + runtime partition set) and the source
    /// releases it.
    pub(crate) partition_adoption:
        SMutex<Option<Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>>>,
    /// In-flight fenced partition-RPC writes per partition (phase 3):
    /// handlers register on entry — before the fence check — so the
    /// cutover's drain observes every decider; the source owner is
    /// quiescent for the partition once the fence is up, no in-doubt
    /// prepare touches it, and this count is zero.
    pub(crate) migration_inflight: Arc<SMutex<std::collections::HashMap<OID, usize>>>,
    /// Read-your-writes watermark of the sessionless begin/commit path
    /// (`XContract::begin_tx`/`commit_tx` — procedures and internal
    /// statements): the oracle current epoch captured at the last
    /// successful commit; the next begin waits it stable before
    /// registering. Session-bound statements track their own per-session
    /// watermark in the worker's session manager instead.
    pub(crate) last_commit_epoch: AtomicU64,
    // commit_gate: AsyncMutex<()>,
}

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only contract.
pub type IoUringXContract = WorkerXContract;
