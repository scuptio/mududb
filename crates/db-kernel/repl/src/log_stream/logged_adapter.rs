//! Connects the common durable log to a business-owned atomic apply transaction.
use super::state::invalid;
use super::{DurableStreamLog, StreamAdapter, StreamMetrics, StreamStruct};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_raft::stream::{StreamDescriptor, StreamProgress};
use std::sync::Arc;

/// Business state persisted atomically with its stream position and dedup result.
/// Implementations must survive cancellation and replay without duplicating
/// effects; a successful return means the business transaction is durable.
#[async_trait]
pub trait StreamStateMachine: Send + Sync {
    /// Recovers the last durably applied prefix from the business transaction.
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>>;
    /// Applies one operation and persists its request result and prefix atomically.
    async fn apply_once(&self, request_id: u128, record: StreamStruct) -> RS<()>;
    /// Captures complete business state at the exact applied cut. Implementations
    /// must serialize against mutations outside the apply scheduler as well.
    async fn snapshot_at(&self, _prefix: Option<&StreamProgress>) -> RS<Vec<u8>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "business state machine has no complete snapshot implementation"
        ))
    }

    /// Installs a previously captured business image at exactly this prefix.
    /// Called during offline group restore before the machine admits requests;
    /// the durable external log is restored separately by the caller. A partial
    /// install must remain retryable, and an identical retry must be accepted
    /// while a conflicting target is rejected. The default fails closed for
    /// machines that have no complete restore implementation.
    async fn restore_at(&self, _prefix: Option<&StreamProgress>, _business: &[u8]) -> RS<()> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "business state machine has no restore implementation"
        ))
    }
}

/// Common storage/apply bridge reusable for data, catalog, epoch and custom
/// streams. All payload checks and prefix reads use the same external log.
pub struct LoggedStreamAdapter {
    log: Arc<DurableStreamLog>,
    business: Arc<dyn StreamStateMachine>,
}
impl LoggedStreamAdapter {
    /// Binds one business state machine to its immutable external log identity.
    pub fn new(log: Arc<DurableStreamLog>, business: Arc<dyn StreamStateMachine>) -> Self {
        Self { log, business }
    }
}
#[async_trait]
impl StreamAdapter for LoggedStreamAdapter {
    fn descriptor(&self) -> &StreamDescriptor {
        self.log.descriptor()
    }
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        self.business.applied_frontier().await
    }
    async fn next_committed(
        &self,
        after: Option<&StreamProgress>,
        committed: &StreamProgress,
    ) -> RS<Option<StreamStruct>> {
        if !self.log.contains(committed).await? {
            return Err(invalid("committed data missing from local log"));
        }
        let next = if let Some(after) = after {
            if !self.log.contains(after).await? || after.end_lsn > committed.end_lsn {
                return Err(invalid("invalid applied history"));
            }
            if after.end_lsn == committed.end_lsn {
                return Ok(None);
            }
            after
                .end_lsn
                .checked_add(1)
                .ok_or_else(|| invalid("stream LSN exhausted"))?
        } else {
            0
        };
        Ok(Some(self.log.record(next).await?.1))
    }
    async fn apply_committed(&self, record: StreamStruct) -> RS<()> {
        let (request_id, durable) = self.log.record(record.progress.end_lsn).await?;
        if durable.identity != record.identity
            || durable.progress != record.progress
            || durable.payload != record.payload
            || durable.dependencies != record.dependencies
        {
            return Err(invalid("apply record differs from durable log"));
        }
        let result = self.business.apply_once(request_id, record).await;
        if result.is_ok() {
            StreamMetrics::global().note_apply(
                self.log.identity().group_id,
                self.log.descriptor().stream_id,
            );
        }
        result
    }
    async fn snapshot_at(&self, prefix: Option<&StreamProgress>) -> RS<Vec<u8>> {
        if self.business.applied_frontier().await?.as_ref() != prefix {
            return Err(invalid(
                "business snapshot prefix differs from apply frontier",
            ));
        }
        if let Some(prefix) = prefix {
            if !self.log.contains(prefix).await? {
                return Err(invalid(
                    "business snapshot prefix lacks durable log history",
                ));
            }
        }
        let business = self.business.snapshot_at(prefix).await?;
        let log = self.log.capture_snapshot(prefix).await?;
        super::LoggedStreamSnapshot::encode(log, business)
    }

    async fn contains_applied(&self, prefix: &StreamProgress) -> RS<bool> {
        let Some(applied) = self.business.applied_frontier().await? else {
            return Ok(false);
        };
        if applied.stream_id != prefix.stream_id
            || applied.stream_generation != prefix.stream_generation
            || applied.end_lsn < prefix.end_lsn
        {
            return Ok(false);
        }
        Ok(self.log.contains(&applied).await? && self.log.contains(prefix).await?)
    }

    fn pin_read(&self, lsn: u64) -> Option<super::ReadFloorGuard> {
        Some(self.log.pin_read(lsn))
    }
}

impl LoggedStreamAdapter {
    /// Restores a complete logged snapshot into an offline target: the external
    /// log first, then the business image at the exact applied prefix. Both
    /// halves are idempotent so an interrupted install can resume, and a target
    /// with a conflicting history or business state fails closed. Business
    /// restore goes through the state machine; a machine without a restore
    /// implementation is rejected rather than silently left half-installed.
    pub async fn restore_snapshot(&self, image: &super::LoggedStreamSnapshot) -> RS<()> {
        image.log().validate_for(
            self.log.identity(),
            self.log.descriptor(),
            image.log().prefix(),
        )?;
        self.log.restore_snapshot(image.log()).await?;
        self.business
            .restore_at(image.log().prefix(), image.business())
            .await
    }
}
