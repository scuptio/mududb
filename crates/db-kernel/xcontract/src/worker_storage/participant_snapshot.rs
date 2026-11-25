//! Participant state at a business snapshot cut. The caller must also capture
//! rows, the WAL cut and group identity before releasing the capture guard.
use super::{participant::ParticipantState, WorkerStorage};
use catalog::contract::tx_decision::{CoordinatorRef, TxDecision};
use mudu::common::{id::OID, result::RS};
use mudu::{error::ErrorCode, mudu_error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

type States = BTreeMap<(CoordinatorRef, OID), ParticipantState>;

/// Versioned participant state, including invisible prepares and final results.
/// It must travel inside a checked worker/group snapshot manifest; on its own
/// it neither identifies the local worker nor authorizes WAL reclamation.
#[derive(Clone, Serialize, Deserialize)]
pub struct ParticipantSnapshot {
    version: u32,
    // A vector lets validation detect duplicate keys instead of silently
    // accepting the last value while deserializing a map.
    entries: Vec<((CoordinatorRef, OID), ParticipantState)>,
}

impl Default for ParticipantSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

impl ParticipantSnapshot {
    /// Validates an image before any snapshot files or live state are changed.
    pub fn validate(&self) -> RS<()> {
        self.validated_states().map(|_| ())
    }

    fn validated_states(&self) -> RS<States> {
        if self.version != 1 {
            return Err(invalid("unsupported participant snapshot version"));
        }
        let mut states = BTreeMap::new();
        for (key, state) in &self.entries {
            validate_state(key, state)?;
            if states.insert(key.clone(), state.clone()).is_some() {
                return Err(invalid("duplicate participant snapshot identity"));
            }
        }
        Ok(states)
    }
}

/// Holds participant apply and new WAL reservations out of a snapshot cut.
/// Keep this guard until the corresponding rows and manifest are captured.
/// Also excludes ordinary committed row apply and WAL replay. The caller
/// still owns the schema/placement and WAL-prefix consistency barrier.
pub struct ParticipantSnapshotGuard<'a> {
    storage: &'a WorkerStorage,
    _writes: mudu_sys::tokio::sync::RwLockWriteGuard<'a, ()>,
    _apply: mudu_sys::tokio::sync::MutexGuard<'a, ()>,
    snapshot: ParticipantSnapshot,
}

impl ParticipantSnapshotGuard<'_> {
    /// Local recovery clocks at the captured row/participant cut.
    pub fn recovery_watermarks(&self) -> RS<super::RecoveryWatermarks> {
        Ok(self.storage.snapshot_cut.lock()?.watermarks)
    }

    /// Seeds recovery clocks only after matching snapshot rows are restored.
    /// This does not restore a WAL cut certificate or authorize serving.
    pub fn restore_recovery_watermarks(&self, water: super::RecoveryWatermarks) -> RS<()> {
        self.storage.observe_recovery_watermarks(water)
    }

    /// Restores participant state while retaining the same exclusive row cut.
    /// Replacing different live history still requires a group install protocol.
    pub fn restore_participants(&self, snapshot: &ParticipantSnapshot) -> RS<()> {
        let restored = snapshot.validated_states()?;
        let mut states = self.storage.participant_states.lock()?;
        if !states.is_empty() && *states != restored {
            return Err(invalid("cannot replace existing participant history"));
        }
        if !restored.is_empty() {
            self.storage.note_participant_wal();
        }
        self.storage.snapshot_cut.lock()?.wal_resume = None;
        *states = restored;
        Ok(())
    }

    /// Starts tracking an installation before the first live mutation. Keep
    /// this attempt until files, in-memory state and WAL bookkeeping succeed.
    /// Validation/download failures before this point do not fence capture.
    pub fn begin_install(&self) -> RS<super::SnapshotInstallAttempt<'_>> {
        self.storage.begin_snapshot_install_owned()
    }

    /// Requires proof that the captured rows cover exactly this exclusive WAL
    /// end. A durable append, older frontier, or later apply is insufficient.
    pub fn require_wal_resume(&self, wal_resume: u64) -> RS<()> {
        if self.storage.snapshot_cut.lock()?.wal_resume != Some(wal_resume) {
            return Err(invalid("snapshot WAL cut has not been fully applied"));
        }
        Ok(())
    }

    /// The immutable participant image protected by this guard.
    pub fn snapshot(&self) -> &ParticipantSnapshot {
        &self.snapshot
    }

    /// Installs the KV image while retaining exclusive ownership of row apply.
    /// The caller must have validated the matching manifest before installation.
    pub async fn restore_kv_latest(&self, entries: Vec<super::KvDumpEntry>) -> RS<()> {
        self.storage.restore_kv_latest_owned(entries).await
    }

    /// Reopens installed relation files without releasing the apply barrier.
    /// The caller must replace files only while holding this same guard.
    pub async fn reload_relations_from_disk(&self) -> RS<()> {
        self.storage.reload_relations_from_disk_owned().await
    }
}

impl Drop for ParticipantSnapshotGuard<'_> {
    fn drop(&mut self) {
        self.storage
            .participant_snapshotting
            .store(false, Ordering::Release);
    }
}

impl WorkerStorage {
    /// Whether interrupted row apply or snapshot installation requires recovery.
    /// A successful later mutation cannot clear this conservative latch.
    pub fn requires_recovery(&self) -> bool {
        self.snapshot_apply_failed.load(Ordering::Acquire)
    }

    /// Captures fully applied participant records. Unknown WAL intent and a
    /// partially applied Commit fail closed; retry after apply/recovery.
    pub async fn capture_participant_snapshot(&self) -> RS<ParticipantSnapshotGuard<'_>> {
        let writes = self.snapshot_apply.write().await;
        if self.snapshot_apply_failed.load(Ordering::Acquire) {
            return Err(invalid(
                "interrupted row apply requires recovery before snapshot",
            ));
        }
        let apply = self.participant_apply.lock().await;
        let inflight = self.participant_inflight.lock()?;
        if !inflight.is_empty() {
            return Err(invalid("participant WAL intent has no completed apply"));
        }
        let snapshot = ParticipantSnapshot {
            version: 1,
            entries: self
                .participant_states
                .lock()?
                .iter()
                .map(|(key, state)| (key.clone(), state.clone()))
                .collect(),
        };
        snapshot.validate()?;
        // reserve_participant_wal checks this while holding the same inflight
        // lock, so no intent can slip between validation and publication.
        self.participant_snapshotting.store(true, Ordering::Release);
        Ok(ParticipantSnapshotGuard {
            storage: self,
            _writes: writes,
            _apply: apply,
            snapshot,
        })
    }

    /// Restores a validated image into an empty participant store. Exact retry
    /// is allowed; replacing different live history requires a group install
    /// protocol and is rejected here. Restore matching rows before serving.
    pub async fn restore_participant_snapshot(&self, snapshot: &ParticipantSnapshot) -> RS<()> {
        let restored = snapshot.validated_states()?;
        let _writes = self.snapshot_apply.write().await;
        self.snapshot_cut.lock()?.wal_resume = None;
        let _apply = self.participant_apply.lock().await;
        let inflight = self.participant_inflight.lock()?;
        if !inflight.is_empty() {
            return Err(invalid("cannot restore over participant WAL intent"));
        }
        let mut states = self.participant_states.lock()?;
        if !states.is_empty() && *states != restored {
            return Err(invalid("cannot replace existing participant history"));
        }
        if !restored.is_empty() {
            self.note_participant_wal();
        }
        *states = restored;
        Ok(())
    }
}

fn validate_state(key: &(CoordinatorRef, OID), state: &ParticipantState) -> RS<()> {
    if let Some(prepare) = &state.prepare {
        if prepare.coordinator != key.0
            || prepare.partition_id != key.1
            || prepare.write_set.is_empty()
            || prepare
                .write_set
                .iter()
                .any(|write| write.partition_id() != key.1)
        {
            return Err(invalid("participant snapshot prepare identity mismatch"));
        }
    }
    match state.decision {
        None if state.prepare.is_some() && !state.applied => Ok(()),
        Some(TxDecision::Aborted) if state.applied => Ok(()),
        Some(TxDecision::Committed { commit_epoch }) if state.applied => {
            if state
                .prepare
                .as_ref()
                .is_some_and(|p| p.snapshot_epoch <= commit_epoch)
            {
                Ok(())
            } else {
                Err(invalid("participant snapshot commit has no valid prepare"))
            }
        }
        _ => Err(invalid("participant snapshot contains incomplete apply")),
    }
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}

#[cfg(test)]
mod tests;
