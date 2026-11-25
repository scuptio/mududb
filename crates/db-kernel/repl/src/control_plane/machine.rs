//! Catalog business effects and request lookup for the metadata control stream.
//!
//! Apply is serialized internally as well as by the scheduler. The injected MetaApplier persists
//! catalog mutations before the durable frontier advances; request lookups are
//! rebuilt from retained external log records on restart.

use crate::log_stream::frontier::{decode, encode};
use crate::log_stream::FrontierStore;
use crate::log_stream::{DurableStreamLog, StreamStateMachine, StreamStruct};
use async_trait::async_trait;
use catalog::contract::control_plane::CONTROL_STREAM_META;
use catalog::contract::epoch_op::EpochOp;
use catalog::contract::meta_op::MetaOp;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{StreamId, StreamProgress};
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

/// Applies a committed metadata operation to the node's local catalog
/// (injected by the server, which owns the meta manager). The second
/// argument is the op's control-stream LSN: it orders placement-affecting
/// ops for the routing overlay and replaces the old meta-group raft index
/// as the placement version.
#[async_trait]
pub trait MetaApplier: Send + Sync {
    /// Checks durable business state against the applied and authoritative
    /// committed prefixes before startup exposes this machine. Implementations
    /// must allow only provable interrupted apply, and must not mutate storage.
    async fn validate_recovery(
        &self,
        log: &DurableStreamLog,
        applied: Option<&StreamProgress>,
        committed: Option<&StreamProgress>,
    ) -> RS<()>;

    /// Applies one committed metadata operation; must be idempotent.
    async fn apply(&self, op: MetaOp, version: u64) -> RS<()>;
}

/// Catalog-stream state machine: applies committed [`MetaOp`]s to the local
/// catalog and maintains the request
/// index powering cross-restart idempotent result lookups. A cancelled or failed
/// business apply/frontier publication fences further mutations until reopen.
/// The caller remains responsible for committed/dependency authorization; local
/// durable record validation alone does not prove quorum commitment.
pub struct MetaStreamMachine {
    frontier: FrontierStore,
    log: Arc<DurableStreamLog>,
    // Serial owner; true means a business/persistence await did not complete.
    // Cancellation leaves it set, so reopening is required before more apply.
    owner: mudu_sys::tokio::sync::Mutex<bool>,
    failed: mudu_sys::tokio::sync::watch::Sender<bool>,
    applied: SMutex<Option<StreamProgress>>,
    applier: Arc<dyn MetaApplier>,
    /// Every applied record's request id → its stream LSN; rebuilt from the
    /// durable log at open. Lets a retry of an already-applied request
    /// resolve without a second apply.
    requests: SMutex<HashMap<u128, u64>>,
    /// Fires whenever the applied frontier advances (result waiters).
    applied_notify: mudu_sys::tokio::sync::Notify,
}

impl MetaStreamMachine {
    /// Opens the machine: recovers the durable applied frontier and rebuilds
    /// the volatile views (request index) by rescanning the
    /// durable stream log. The catalog itself is durable (idempotent ops are
    /// never re-applied).
    pub async fn open(
        log: &Arc<DurableStreamLog>,
        applier: Arc<dyn MetaApplier>,
        apply_dir: &std::path::Path,
        committed: Option<&StreamProgress>,
    ) -> RS<Self> {
        let (frontier, recovered) =
            FrontierStore::open(apply_dir, "meta.frontier", StreamId(CONTROL_STREAM_META), 1)
                .await?;
        if log.descriptor() != &super::streams::meta_stream_descriptor() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "metadata recovery stream mismatch"
            ));
        }
        let mut requests = HashMap::new();
        if let Some((progress, business)) = &recovered {
            let covered = committed.is_some_and(|cut| {
                cut.stream_id == progress.stream_id
                    && cut.stream_generation == progress.stream_generation
                    && (cut.end_lsn > progress.end_lsn || cut == progress)
            });
            if !covered || !business.is_empty() {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "metadata frontier exceeds committed history or has an unsupported checkpoint"
                ));
            }
            if !log.contains(progress).await? {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "meta applied frontier is not covered by the durable stream log"
                ));
            }
            for lsn in 0..=progress.end_lsn {
                let (request_id, record) = log.record(lsn).await?;
                requests.insert(request_id, lsn);
                let _: MetaOp = decode(&record.payload)?;
            }
        }
        applier
            .validate_recovery(log, recovered.as_ref().map(|(p, _)| p), committed)
            .await?;
        Ok(Self {
            frontier,
            log: log.clone(),
            owner: mudu_sys::tokio::sync::Mutex::new(false),
            failed: mudu_sys::tokio::sync::watch::channel(false).0,
            applied: SMutex::new(recovered.map(|(progress, _)| progress)),
            applier,
            requests: SMutex::new(requests),
            applied_notify: mudu_sys::tokio::sync::Notify::new(),
        })
    }

    /// Whether the request's record is durable and already applied locally.
    /// A retry of a committed request resolves here instead of re-applying.
    pub fn request_applied(&self, request_id: u128) -> RS<bool> {
        let requests = self
            .requests
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "meta requests lock poisoned"))?;
        let Some(lsn) = requests.get(&request_id) else {
            return Ok(false);
        };
        let applied = self
            .applied
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "meta applied lock poisoned"))?;
        Ok(applied.as_ref().is_some_and(|p| p.end_lsn >= *lsn))
    }

    /// Wakes whenever the applied frontier advances.
    pub fn applied_notify(&self) -> &mudu_sys::tokio::sync::Notify {
        &self.applied_notify
    }
}

#[async_trait]
impl StreamStateMachine for MetaStreamMachine {
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(self.applied.lock()?.clone())
    }

    async fn apply_once(&self, request_id: u128, record: StreamStruct) -> RS<()> {
        let mut interrupted = self.owner.lock().await;
        if *interrupted {
            return Err(invalid(
                "metadata apply was interrupted; reopen before retrying",
            ));
        }
        self.ensure_available()?;
        if !self.validate_record(request_id, &record).await? {
            return Ok(());
        }
        let op: MetaOp = decode(&record.payload)?;
        *interrupted = true;
        let attempt = self.failure_guard();
        if let Err(error) = self.applier.apply(op, record.progress.end_lsn).await {
            warn!(request_id, "control meta op apply failed: {}", error);
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "control meta stream apply failed"
            ));
        }
        self.frontier.store(&record.progress, &[]).await?;
        self.requests
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "meta requests lock poisoned"))?
            .insert(request_id, record.progress.end_lsn);
        *self
            .applied
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "meta applied lock poisoned"))? =
            Some(record.progress);
        *interrupted = false;
        attempt.complete();
        self.applied_notify.notify_waiters();
        Ok(())
    }
}

/// Encodes one catalog op as a meta-stream payload.
pub(crate) fn encode_meta_op(op: &MetaOp) -> RS<Vec<u8>> {
    encode(op)
}

/// Encodes one epoch op as an epoch-stream payload.
pub(crate) fn encode_epoch_op(op: &EpochOp) -> RS<Vec<u8>> {
    encode(op)
}

mod apply_guard;
mod result;

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}
