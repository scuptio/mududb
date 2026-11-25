//! Row-apply ownership and evidence for an exact, fully applied WAL cut.
use super::WorkerStorage;
use mudu::common::result::RS;
use mudu::{error::ErrorCode, mudu_error};
use std::sync::atomic::Ordering;

#[derive(Default)]
pub(super) struct ApplyCut {
    generation: u64,
    active: u64,
    overlapped: bool,
    pub(super) wal_resume: Option<u64>,
    pub(super) watermarks: super::RecoveryWatermarks,
}

pub(super) struct ApplyAttempt<'a> {
    _permit: mudu_sys::tokio::sync::RwLockReadGuard<'a, ()>,
    storage: &'a WorkerStorage,
    generation: u64,
    complete: bool,
}

/// Tracks a snapshot installation while its caller holds exclusive row ownership.
/// Dropping without successful completion fences future snapshot capture. This
/// is an in-process failure latch, not a crash-recoverable installation journal.
#[must_use = "complete the installation only after all state and WAL changes succeed"]
pub struct SnapshotInstallAttempt<'a> {
    storage: &'a WorkerStorage,
    complete: bool,
}

impl SnapshotInstallAttempt<'_> {
    /// Marks the whole installation successful. Does not clear a failure from
    /// a nested restore or any earlier attempt.
    pub fn complete(mut self) {
        self.complete = true;
    }
}

impl Drop for SnapshotInstallAttempt<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.storage
                .snapshot_apply_failed
                .store(true, Ordering::Release);
        }
    }
}

impl ApplyAttempt<'_> {
    pub(super) fn finish<T>(self, result: RS<T>) -> RS<T> {
        self.finish_at(result, None)
    }

    pub(super) fn finish_at<T>(mut self, result: RS<T>, wal_resume: Option<u64>) -> RS<T> {
        if result.is_ok() {
            let mut cut = self.storage.snapshot_cut.lock()?;
            // A concurrent or later mutation invalidates this attempt's
            // certificate. Waiting for all readers alone cannot fix its LSN.
            if cut.active == 1 && !cut.overlapped && cut.generation == self.generation {
                cut.wal_resume = wal_resume;
            }
            self.complete = true;
        }
        result
    }
}

impl Drop for ApplyAttempt<'_> {
    fn drop(&mut self) {
        if let Ok(mut cut) = self.storage.snapshot_cut.lock() {
            cut.active -= 1;
            if !self.complete {
                cut.wal_resume = None;
            }
        } else {
            self.storage
                .snapshot_apply_failed
                .store(true, Ordering::Release);
        }
        if !self.complete {
            self.storage
                .snapshot_apply_failed
                .store(true, Ordering::Release);
        }
    }
}

impl WorkerStorage {
    // The caller must hold snapshot_apply exclusively through this attempt.
    pub(super) fn begin_snapshot_install_owned(&self) -> RS<SnapshotInstallAttempt<'_>> {
        self.snapshot_cut.lock()?.wal_resume = None;
        Ok(SnapshotInstallAttempt {
            storage: self,
            complete: false,
        })
    }

    pub(super) async fn begin_snapshot_apply(&self) -> RS<ApplyAttempt<'_>> {
        let permit = self.snapshot_apply.read().await;
        let mut cut = self.snapshot_cut.lock()?;
        let generation = cut
            .generation
            .checked_add(1)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "apply generation exhausted"))?;
        let active = cut
            .active
            .checked_add(1)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "apply counter exhausted"))?;
        cut.generation = generation;
        // Remember overlap until the whole cohort drains, including when
        // the latest entrant is the last to finish.
        cut.overlapped = cut.active != 0;
        cut.active = active;
        cut.wal_resume = None;
        Ok(ApplyAttempt {
            _permit: permit,
            storage: self,
            generation,
            complete: false,
        })
    }
}
