//! Participant apply is serialized and shared by leader commit and WAL replay.
use super::WorkerStorage;
use catalog::contract::tx_decision::TxDecision;
use mudu::common::{id::OID, result::RS};
use mudu::{error::ErrorCode, mudu_error};
use wal::participant::{ParticipantDecision, ParticipantPrepare};
use wal::xl_batch::XLBatch;
use wal::xl_entry::{TxOp, XLEntry};

#[derive(Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct ParticipantState {
    pub(super) prepare: Option<ParticipantPrepare>,
    pub(super) decision: Option<TxDecision>,
    pub(super) applied: bool,
}

impl WorkerStorage {
    /// Participant history must remain in WAL until participant-aware
    /// snapshots include both unresolved writes and completed results.
    pub fn has_participant_history(&self) -> RS<bool> {
        Ok(self
            .participant_wal_seen
            .load(std::sync::atomic::Ordering::Acquire)
            || !self.participant_states.lock()?.is_empty())
    }

    pub(crate) fn pending_participant_records(&self) -> RS<Vec<ParticipantPrepare>> {
        Ok(self
            .participant_states
            .lock()?
            .values()
            .filter(|state| !state.applied)
            .filter_map(|state| state.prepare.clone())
            .collect())
    }

    pub(crate) fn reserve_participant_wal(&self, partition: OID) -> RS<()> {
        let mut inflight = self.participant_inflight.lock()?;
        if self
            .participant_snapshotting
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(invalid("participant snapshot capture is in progress"));
        }
        if !inflight.insert(partition) {
            return Err(invalid(
                "participant WAL result is still unknown; wait for apply or recovery",
            ));
        }
        self.note_participant_wal();
        Ok(())
    }

    pub(crate) fn note_participant_wal(&self) {
        self.participant_wal_seen
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub(crate) fn ensure_partition_not_prepared(&self, partition: OID) -> RS<()> {
        if self.participant_pending(partition)? {
            return Err(invalid("partition has an unresolved durable prepare"));
        }
        Ok(())
    }

    pub(crate) fn participant_pending(&self, partition: OID) -> RS<bool> {
        if self.participant_inflight.lock()?.contains(&partition) {
            return Ok(true);
        }
        Ok(self
            .participant_states
            .lock()?
            .iter()
            .any(|((_, id), state)| *id == partition && state.prepare.is_some() && !state.applied))
    }

    /// True for an identical durable prepare; conflicting retries fail closed.
    pub(crate) fn check_participant_prepare(&self, record: &ParticipantPrepare) -> RS<bool> {
        if record.write_set.is_empty()
            || record
                .write_set
                .iter()
                .any(|write| write.partition_id() != record.partition_id)
        {
            return Err(invalid(
                "prepare must contain only its participant partition",
            ));
        }
        let states = self.participant_states.lock()?;
        let Some(state) = states.get(&(record.coordinator.clone(), record.partition_id)) else {
            return Ok(false);
        };
        if state.decision == Some(TxDecision::Aborted) {
            return Err(invalid("participant already aborted"));
        }
        if state.prepare.as_ref() != Some(record) {
            return Err(invalid("conflicting participant prepare"));
        }
        Ok(true)
    }

    /// Validate before writing so a rejected command cannot poison WAL apply.
    pub(crate) fn check_participant_decision(&self, record: &ParticipantDecision) -> RS<bool> {
        let states = self.participant_states.lock()?;
        let state = states.get(&(record.coordinator.clone(), record.partition_id));
        if let Some(previous) = state.and_then(|state| state.decision) {
            return if previous == record.decision {
                Ok(state.is_some_and(|state| state.applied))
            } else {
                Err(invalid("conflicting participant decision"))
            };
        }
        if let TxDecision::Committed { commit_epoch } = record.decision {
            let prepare = state
                .and_then(|state| state.prepare.as_ref())
                .ok_or_else(|| invalid("commit requires a durable participant prepare"))?;
            if commit_epoch < prepare.snapshot_epoch {
                return Err(invalid("participant commit precedes its snapshot"));
            }
        }
        Ok(false)
    }

    pub(super) async fn apply_participant_op(&self, op: TxOp) -> RS<()> {
        self.note_participant_wal();
        let _owner = self.participant_apply.lock().await;
        let partition = match &op {
            TxOp::Prepare(record) => record.partition_id,
            TxOp::ParticipantDecision(record) => record.partition_id,
            _ => return Err(invalid("expected participant WAL record")),
        };
        match op {
            TxOp::Prepare(record) => {
                if !self.check_participant_prepare(&record)? {
                    self.participant_states.lock()?.insert(
                        (record.coordinator.clone(), record.partition_id),
                        ParticipantState {
                            prepare: Some(record),
                            decision: None,
                            applied: false,
                        },
                    );
                }
            }
            TxOp::ParticipantDecision(record) => self.apply_participant_decision(record).await?,
            _ => return Err(invalid("expected participant WAL record")),
        }
        self.participant_inflight.lock()?.remove(&partition);
        Ok(())
    }

    async fn apply_participant_decision(&self, record: ParticipantDecision) -> RS<()> {
        if self.check_participant_decision(&record)? {
            return Ok(());
        }
        let key = (record.coordinator.clone(), record.partition_id);
        // This record is already quorum committed. Bind its immutable outcome
        // before applying rows, even if business apply later fails or is cancelled.
        self.participant_states
            .lock()?
            .entry(key.clone())
            .or_default()
            .decision = Some(record.decision);
        if let TxDecision::Committed { commit_epoch } = record.decision {
            let prepare = self
                .participant_states
                .lock()?
                .get(&key)
                .and_then(|state| state.prepare.clone())
                .ok_or_else(|| invalid("participant prepare disappeared"))?;
            // Reuse ordinary absolute WAL replay. If apply fails or is cancelled,
            // retain the prepare; retry repeats writes at the same version.
            let mut ops = vec![TxOp::Begin];
            ops.extend(prepare.write_set.into_iter().map(TxOp::Write));
            ops.push(TxOp::Commit(Default::default()));
            self.replay_regular_batch(XLBatch::new(vec![XLEntry {
                xid: record.coordinator.tx_id as u64,
                ops,
                commit_epoch: Some(commit_epoch),
            }]))
            .await?;
        }
        self.participant_states
            .lock()?
            .entry(key.clone())
            .or_default()
            .applied = true;
        Ok(())
    }
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::Transaction, message)
}

#[cfg(test)]
#[path = "participant_tests.rs"]
mod tests;
