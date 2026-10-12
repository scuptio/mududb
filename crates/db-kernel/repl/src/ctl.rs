//! Replication-mode (non-raft) control for the write gate and the primary's
//! synchronous commit path.
//!
//! In stream mode (`[replication.stream]`) there is no raft group: the
//! configured primary commits locally (and optionally waits for a
//! replica's durable ack), the replicas apply the stream immediately and
//! stay read-only for clients.

use crate::{ack_watermarks, ReplWorkerState};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::LeaderHint;
use mudu_sys::sync::SRwLock;
use mudu_sys::tokio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Replication-mode control attached to every worker of a node running
/// in stream mode. One shared instance per node; role/primary changes are
/// visible to every worker immediately (the write gate re-reads it per
/// request, the sink re-evaluates its target every round).
#[derive(Clone)]
pub struct ReplModeCtl {
    inner: Arc<SRwLock<CtlInner>>,
    /// Current stream epoch: feeds `xid = (epoch << 32) | counter` and the
    /// replication protocol's epoch checks. Atomic because xid allocation
    /// is a hot path.
    epoch: Arc<AtomicU64>,
    applied_watermark: Arc<AppliedWatermark>,
}

struct CtlInner {
    is_primary: bool,
    /// The primary this node follows (replica role).
    primary_node_id: Option<u64>,
    /// Replication address of the followed primary (replica role).
    primary_addr: Option<String>,
    /// Client redirect hint pointing at the primary (replica role).
    leader_hint: Option<LeaderHint>,
    sync: bool,
    sync_timeout: Duration,
}

/// The stream primary's applied low-water mark: the highest LSN such that
/// every commit up to it finished applying. Snapshot points on the primary
/// are taken at this watermark — never at the WAL allocator, which can run
/// ahead of in-flight applies.
///
/// Commit paths bracket their apply with [`AppliedWatermark::begin`] /
/// [`AppliedWatermark::end`]; the water only advances when no apply is in
/// flight, to the highest allocated LSN at that moment, so any LSN it
/// covers was fully applied by construction.
#[derive(Default)]
pub(crate) struct AppliedWatermark {
    in_flight: std::sync::atomic::AtomicU64,
    allocated: std::sync::atomic::AtomicU64,
    water: std::sync::atomic::AtomicU64,
}

impl AppliedWatermark {
    /// Marks the start of one commit's append/apply critical span.
    pub(crate) fn begin(&self) {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
    }

    /// Marks the end of the span (success or failure); `allocated_lsn` is
    /// the WAL allocator's high-water after the commit's enqueue.
    pub(crate) fn end(&self, allocated_lsn: u64) {
        self.allocated.fetch_max(allocated_lsn, Ordering::SeqCst);
        if self.in_flight.fetch_sub(1, Ordering::SeqCst) == 1 {
            let allocated = self.allocated.load(Ordering::SeqCst);
            self.water.fetch_max(allocated, Ordering::SeqCst);
        }
    }

    /// The applied low-water mark (0 before the first quiescent point).
    pub(crate) fn water(&self) -> u64 {
        self.water.load(Ordering::SeqCst)
    }
}

impl ReplModeCtl {
    /// Creates the control with an explicit role.
    pub fn with_role(
        is_primary: bool,
        primary_node_id: Option<u64>,
        primary_addr: Option<String>,
        leader_hint: Option<LeaderHint>,
        sync: bool,
        sync_timeout: Duration,
        epoch: u64,
    ) -> Self {
        Self {
            inner: Arc::new(SRwLock::new(CtlInner {
                is_primary,
                primary_node_id,
                primary_addr,
                leader_hint,
                sync,
                sync_timeout,
            })),
            epoch: Arc::new(AtomicU64::new(epoch)),
            applied_watermark: Arc::new(AppliedWatermark::default()),
        }
    }

    fn read_inner(&self) -> RS<mudu_sys::sync::SRwLockReadGuard<'_, CtlInner>> {
        self.inner
            .read()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ctl lock poisoned"))
    }

    fn write_inner(&self) -> RS<mudu_sys::sync::SRwLockWriteGuard<'_, CtlInner>> {
        self.inner
            .write()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ctl lock poisoned"))
    }

    /// Whether this node is currently the primary.
    pub fn is_primary(&self) -> bool {
        self.read_inner()
            .map(|inner| inner.is_primary)
            .unwrap_or(false)
    }

    /// Whether the primary waits for a replica's durable ack before
    /// answering a commit.
    pub fn sync(&self) -> bool {
        self.read_inner().map(|inner| inner.sync).unwrap_or(false)
    }

    /// The synchronous replication wait timeout.
    pub(crate) fn sync_timeout(&self) -> Duration {
        self.read_inner()
            .map(|inner| inner.sync_timeout)
            .unwrap_or(Duration::from_millis(10000))
    }

    /// Leader hint pointing at the current primary (replica role).
    pub(crate) fn leader_hint(&self) -> Option<LeaderHint> {
        self.read_inner()
            .ok()
            .and_then(|inner| inner.leader_hint.clone())
    }

    /// The primary this node follows (replica role).
    pub(crate) fn primary_node_id(&self) -> Option<u64> {
        self.read_inner()
            .ok()
            .and_then(|inner| inner.primary_node_id)
    }

    /// Replication address of the followed primary (replica role).
    pub fn primary_addr(&self) -> Option<String> {
        self.read_inner()
            .ok()
            .and_then(|inner| inner.primary_addr.clone())
    }

    /// The current stream epoch.
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Relaxed)
    }

    /// The epoch as a shared atomic, feeding the xid allocator
    /// (`xid = (epoch << 32) | counter` in stream mode, so a promoted
    /// primary never reissues the old primary's transaction ids).
    pub(crate) fn epoch_source(&self) -> Arc<AtomicU64> {
        self.epoch.clone()
    }

    /// Records a higher stream epoch observed from a primary (sink).
    pub(crate) fn note_epoch(&self, epoch: u64) {
        self.epoch.fetch_max(epoch, Ordering::Relaxed);
    }

    /// Switches the node to the primary role at `new_epoch` (promote; the
    /// epoch is already persisted by the caller).
    pub(crate) fn set_primary(&self, new_epoch: u64) -> RS<()> {
        let mut inner = self.write_inner()?;
        inner.is_primary = true;
        inner.primary_node_id = None;
        inner.primary_addr = None;
        inner.leader_hint = None;
        drop(inner);
        self.epoch.store(new_epoch, Ordering::Relaxed);
        Ok(())
    }

    /// Switches the node to follow a new primary (follow).
    pub(crate) fn set_follow(
        &self,
        primary_node_id: u64,
        primary_addr: String,
        leader_hint: Option<LeaderHint>,
    ) -> RS<()> {
        let mut inner = self.write_inner()?;
        inner.is_primary = false;
        inner.primary_node_id = Some(primary_node_id);
        inner.primary_addr = Some(primary_addr);
        inner.leader_hint = leader_hint;
        Ok(())
    }

    /// The applied low-water mark of the primary's commit path (snapshot
    /// trigger). Zero on replicas (they snapshot at the sink's resume point).
    pub(crate) fn applied_low_water(&self) -> u64 {
        self.applied_watermark.water()
    }

    /// Brackets one commit's append/apply span (primary commit path).
    pub(crate) fn apply_track_begin(&self) {
        self.applied_watermark.begin();
    }

    /// Closes the span opened by [`Self::apply_track_begin`].
    pub(crate) fn apply_track_end(&self, allocated_lsn: u64) {
        self.applied_watermark.end(allocated_lsn);
    }
}

/// Waits for a replica of one worker to cover a commit point. Shared by the
/// primary's commit paths; notification-driven (wakes on every recorded
/// ack), never a polling hot loop.
#[derive(Clone)]
pub struct ReplAckWaiter {
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    state: Arc<ReplWorkerState>,
}

impl ReplAckWaiter {
    /// Creates the waiter for one worker of one server.
    pub fn new(
        server_instance_id: ServerInstanceId,
        worker_index: u32,
        state: Arc<ReplWorkerState>,
    ) -> Self {
        Self {
            server_instance_id,
            worker_index,
            state,
        }
    }

    /// Blocks until any connected replica of this worker reports
    /// `stored_lsn >= lsn`, or `timeout` elapses. A timeout is a client
    /// error: the commit is durable locally but unconfirmed on replicas.
    pub(crate) async fn wait_cover(&self, lsn: u64, timeout: Duration) -> RS<()> {
        let deadline = *mudu_sys::time::instant_now() + timeout;
        loop {
            let watermarks = ack_watermarks(self.server_instance_id, self.worker_index)?;
            if watermarks.iter().any(|(_, stored)| *stored >= lsn) {
                return Ok(());
            }
            let now = *mudu_sys::time::instant_now();
            if now >= deadline {
                crate::status::note_sync_write_timeout(self.server_instance_id);
                return Err(mudu_error!(
                    ErrorCode::TimedOut,
                    format!(
                        "synchronous replication timeout: no replica has stored lsn {} within {:?}",
                        lsn, timeout
                    )
                ));
            }
            let remaining = deadline - now;
            tokio::select! {
                _ = self.state.ack_notified() => {}
                _ = mudu_sys::task::async_::sleep(remaining) => {
                    crate::status::note_sync_write_timeout(self.server_instance_id);
                    return Err(mudu_error!(
                        ErrorCode::TimedOut,
                        format!(
                            "synchronous replication timeout: no replica has stored lsn {} within {:?}",
                            lsn, timeout
                        )
                    ));
                }
            }
        }
    }
}

impl ::catalog::repl_ctrl::ReplControl for ReplModeCtl {
    fn is_primary(&self) -> bool {
        ReplModeCtl::is_primary(self)
    }

    fn sync(&self) -> bool {
        ReplModeCtl::sync(self)
    }

    fn sync_timeout(&self) -> Duration {
        ReplModeCtl::sync_timeout(self)
    }

    fn leader_hint(&self) -> Option<LeaderHint> {
        ReplModeCtl::leader_hint(self)
    }

    fn epoch(&self) -> u64 {
        ReplModeCtl::epoch(self)
    }

    fn epoch_source(&self) -> Arc<AtomicU64> {
        ReplModeCtl::epoch_source(self)
    }

    fn note_epoch(&self, epoch: u64) {
        ReplModeCtl::note_epoch(self, epoch)
    }

    fn set_primary(&self, new_epoch: u64) -> RS<()> {
        ReplModeCtl::set_primary(self, new_epoch)
    }

    fn set_follow(
        &self,
        primary_node_id: u64,
        primary_addr: String,
        leader_hint: Option<LeaderHint>,
    ) -> RS<()> {
        ReplModeCtl::set_follow(self, primary_node_id, primary_addr, leader_hint)
    }

    fn primary_node_id(&self) -> Option<u64> {
        ReplModeCtl::primary_node_id(self)
    }

    fn primary_addr(&self) -> Option<String> {
        ReplModeCtl::primary_addr(self)
    }

    fn applied_low_water(&self) -> u64 {
        ReplModeCtl::applied_low_water(self)
    }

    fn apply_track_begin(&self) {
        ReplModeCtl::apply_track_begin(self)
    }

    fn apply_track_end(&self, allocated_lsn: u64) {
        ReplModeCtl::apply_track_end(self, allocated_lsn)
    }
}

#[async_trait::async_trait]
impl ::catalog::repl_ctrl::ReplAckWait for ReplAckWaiter {
    async fn wait_cover(&self, lsn: u64, timeout: Duration) -> RS<()> {
        ReplAckWaiter::wait_cover(self, lsn, timeout).await
    }
}
