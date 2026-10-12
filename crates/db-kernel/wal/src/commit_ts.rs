//! Commit timestamp: a hybrid logical clock stamped on every committed
//! transaction, carried by the WAL so point-in-time recovery can translate a
//! wall-clock target into a replay stop point.
//!
//! The value is a single `u64` of milliseconds since the Unix epoch. A
//! process-local logical guard makes consecutive stamps strictly increasing
//! even when the physical clock steps backwards or two commits land in the
//! same millisecond, so a time -> LSN mapping is monotonic within one WAL
//! timeline.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Millis-since-epoch timestamp of a commit, made monotonic per process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CommitTimestamp(pub u64);

impl CommitTimestamp {
    /// The zero timestamp (used as a placeholder in tests and synthetic
    /// batches that carry no real commit time).
    pub const ZERO: CommitTimestamp = CommitTimestamp(0);

    /// The underlying millisecond count since the Unix epoch.
    pub fn as_millis(self) -> u64 {
        self.0
    }
}

impl Default for CommitTimestamp {
    fn default() -> Self {
        CommitTimestamp::ZERO
    }
}

impl From<u64> for CommitTimestamp {
    fn from(millis: u64) -> Self {
        CommitTimestamp(millis)
    }
}

/// The highest logical stamp issued so far in this process. Seeded from the
/// physical clock on first use and bumped past every observed physical time
/// so stamps stay strictly increasing regardless of clock jumps.
fn last_logical_stamp() -> &'static AtomicU64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    &LAST
}

/// Current wall-clock time in milliseconds since the Unix epoch.
fn physical_millis() -> u64 {
    mudu_sys::time::system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Issues the next strictly-increasing commit timestamp: the physical clock
/// advanced by a logical guard so no two stamps collide and none regresses.
pub fn next_commit_timestamp() -> CommitTimestamp {
    let physical = physical_millis();
    let last = last_logical_stamp();
    loop {
        let current = last.load(Ordering::SeqCst);
        let next = physical.max(current.saturating_add(1));
        if last
            .compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return CommitTimestamp(next);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn commit_timestamp_is_strictly_monotonic() {
        let a = next_commit_timestamp();
        let b = next_commit_timestamp();
        assert!(a < b, "{a:?} must be strictly less than {b:?}");
    }

    #[test]
    fn commit_timestamp_round_trips_msgpack() {
        let ts = CommitTimestamp(1_700_000_000_123);
        let encoded = rmp_serde::to_vec(&ts).unwrap();
        let decoded: CommitTimestamp = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(decoded, ts);
        assert_eq!(decoded.as_millis(), 1_700_000_000_123);
    }
}
