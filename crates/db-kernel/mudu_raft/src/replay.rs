//! Bounded reads of the locally durable committed log for service recovery.

use crate::core::message::TermIndex;
use crate::entry::ReplEntry;

/// Approximate owned-entry byte budget of one recovery page. A single larger
/// legacy entry is returned alone so recovery can still make progress.
pub const MAX_REPLAY_BYTES: usize = 4 * 1024 * 1024;

/// One page of locally committed entries. This is a recovery read, not a
/// linearizable read barrier: callers requiring authority first commit a barrier.
#[derive(Clone, Debug)]
pub struct CommittedBatch {
    /// Frontier already compacted out of the log; restore its service snapshot
    /// when the caller's cursor is below this index.
    pub snapshot: TermIndex,
    /// Committed index observed by the serialized driver.
    pub committed_index: u64,
    /// Entries after the requested cursor, in index order, including each term.
    pub entries: Vec<(TermIndex, ReplEntry)>,
}
