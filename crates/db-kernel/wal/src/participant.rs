//! Participant records are self-contained; neither xid truncation nor the
//! worker's volatile RPC dedup cache defines their identity.
use crate::xl_data_op::XLWrite;
use catalog::contract::tx_decision::{CoordinatorRef, TxDecision};
use mudu::common::id::OID;
use serde::{Deserialize, Serialize};

/// The immutable write set and read snapshot of one participant partition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantPrepare {
    /// Stable decision owner and full transaction id, persisted for recovery.
    pub coordinator: CoordinatorRef,
    /// Participant partition, distinguishing multiple partitions on one worker.
    pub partition_id: OID,
    /// Snapshot used for conflict validation.
    pub snapshot_epoch: u64,
    /// Invisible writes retained until an authoritative decision arrives.
    pub write_set: Vec<XLWrite>,
}

/// A durably applied final outcome of one participant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantDecision {
    /// Stable decision owner and full transaction id.
    pub coordinator: CoordinatorRef,
    /// Participant partition.
    pub partition_id: OID,
    /// Immutable outcome; a different retry is rejected.
    pub decision: TxDecision,
    /// Commit clock retained for archive time/XID target selection.
    pub commit_ts: crate::commit_ts::CommitTimestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{commit_ts::CommitTimestamp, xl_entry::TxOp};

    #[test]
    fn archive_clock_includes_committed_participant_decisions_only() {
        let mut record = ParticipantDecision {
            coordinator: CoordinatorRef {
                cluster_id: 1,
                group_id: 1,
                group_generation: 1,
                tx_id: 1,
            },
            partition_id: 2,
            decision: TxDecision::Committed { commit_epoch: 7 },
            commit_ts: CommitTimestamp(123),
        };
        assert_eq!(
            TxOp::ParticipantDecision(record.clone()).commit_ts(),
            Some(CommitTimestamp(123))
        );
        record.decision = TxDecision::Aborted;
        assert_eq!(TxOp::ParticipantDecision(record).commit_ts(), None);
    }
}
