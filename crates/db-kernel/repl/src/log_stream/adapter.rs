//! Business integration contract for applying externally replicated records.

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_raft::stream::{GroupIdobject, StreamDescriptor, StreamProgress};

/// A verified record and the exact applied prefixes it depends on.
#[derive(Clone, Debug)]
pub struct StreamStruct {
    /// Namespace of the external log.
    pub identity: GroupIdobject,
    /// Inclusive position and digest of this record's prefix.
    pub progress: StreamProgress,
    /// Required prefixes in other streams of the same group.
    pub dependencies: Vec<StreamProgress>,
    /// Encoded business operation, bounded by the registered record budget.
    pub payload: Vec<u8>,
}

/// Storage and business apply boundary. Implementations must serialize their
/// mutations and durably deduplicate replayed operations. Cancellation may occur
/// at any await: an operation must either remain unapplied or be recoverable via
/// `applied_frontier`; retrying it must not duplicate its business effects.
#[async_trait]
pub trait StreamAdapter: Send + Sync {
    /// Immutable identity and format supported by this adapter.
    fn descriptor(&self) -> &StreamDescriptor;

    /// Recovers the durable business frontier, including its prefix digest.
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>>;

    /// Reads exactly the next record (LSNs start at zero). Before returning,
    /// verify the local durable prefix through `committed`, including its digest.
    /// Missing bytes must be fetched or return an error; speculative bytes must
    /// never be applied. Return None only when already at the committed frontier.
    async fn next_committed(
        &self,
        after: Option<&StreamProgress>,
        committed: &StreamProgress,
    ) -> RS<Option<StreamStruct>>;

    /// Atomically apply the record and persist its applied frontier and dedup
    /// result. Must return only after both survive restart. Never perform a
    /// cross-stream wait here: the scheduler has already checked dependencies.
    async fn apply_committed(&self, record: StreamStruct) -> RS<()>;

    /// Captures all business state and dedup results at exactly this prefix.
    /// Called under all scheduler apply locks. Implementations must also freeze
    /// writes outside the scheduler and reject unsupported partial images.
    async fn snapshot_at(&self, _prefix: Option<&StreamProgress>) -> RS<Vec<u8>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "stream adapter has no complete snapshot implementation"
        ))
    }

    /// Verifies that the recovered applied history includes this exact prefix,
    /// even when the current frontier is beyond it. Digest equality at a later
    /// LSN is not evidence for an earlier prefix.
    async fn contains_applied(&self, prefix: &StreamProgress) -> RS<bool>;

    /// Pins the external log's read floor so prefix GC cannot reclaim records
    /// at or below `lsn` while the returned guard lives. Adapters without a
    /// reclaimable log return `None`.
    fn pin_read(&self, _lsn: u64) -> Option<super::ReadFloorGuard> {
        None
    }
}
