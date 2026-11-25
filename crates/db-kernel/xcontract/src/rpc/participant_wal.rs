//! Participant control records use exactly the ordinary WAL quorum path.
use super::*;
use crate::kv::commit::stage_autocommit;
use crate::kv::log_cloned;
use storage::prepared_commit::PreparedWorkerCommit;

impl WorkerXContract {
    pub(super) async fn persist_participant_op(&self, op: TxOp) -> RS<()> {
        let log = log_cloned(&self.log)?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "two-phase commit requires durable worker WAL"
            )
        })?;
        let route = self.repl_commit_route()?;
        if matches!(route, ReplCommitRoute::Reject) {
            return Err(not_serving_leader_error());
        }
        let partition = match &op {
            TxOp::Prepare(record) => record.partition_id,
            TxOp::ParticipantDecision(record) => record.partition_id,
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    "expected participant WAL operation"
                ))
            }
        };
        self.storage.reserve_participant_wal(partition)?;
        let commit_epoch = match &op {
            TxOp::ParticipantDecision(wal::participant::ParticipantDecision {
                decision: catalog::contract::tx_decision::TxDecision::Committed { commit_epoch },
                ..
            }) => Some(*commit_epoch),
            _ => None,
        };
        // Coordinator ids are random u128 values, not local MVCC xids. The
        // full identity lives in the payload; do not advance the local xid
        // allocator with a truncated random coordinator id during recovery.
        let batch = XLBatch::new(vec![XLEntry {
            xid: 0,
            ops: vec![op],
            commit_epoch,
        }]);
        match route {
            ReplCommitRoute::Reject => return Err(not_serving_leader_error()),
            ReplCommitRoute::Sequencer(sequencer) => {
                let prepared = PreparedWorkerCommit {
                    xid: 0,
                    commit_epoch,
                    relation_rows: BTreeMap::new(),
                    relation_deltas: BTreeMap::new(),
                    kv_rows: BTreeMap::new(),
                    batch,
                };
                stage_autocommit(&log, &sequencer, prepared).await?;
            }
            route => {
                let frames = log.serialize_entry(&batch)?;
                let lsns = ::wal::log_frame::frame_lsns(&frames)?;
                let last = log.enqueue_group_commit(frames, lsns, true).await?;
                if let ReplCommitRoute::DirectSync(waiter, timeout) = route {
                    waiter.wait_cover(last.as_u64(), timeout).await?;
                }
                self.storage.replay_batch(batch).await?;
            }
        }
        if let Some(epoch) = commit_epoch {
            self.snapshot_mgr.note_applied_commit_epoch(epoch);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "participant_wal_tests.rs"]
mod tests;
