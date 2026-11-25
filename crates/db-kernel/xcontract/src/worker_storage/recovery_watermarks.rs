//! Visibility/allocation evidence captured under the same barrier as rows.
use super::WorkerStorage;
use catalog::contract::tx_decision::TxDecision;
use mudu::common::result::RS;
use serde::{Deserialize, Serialize};
use wal::{xl_batch::XLBatch, xl_entry::TxOp};

/// Monotone clocks of successfully applied row effects. These are local
/// recovery clocks, not proof of global stable epoch or permission to GC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryWatermarks {
    /// Highest transaction identifier observed in fully applied WAL batches.
    pub max_xid: u64,
    /// Highest applied commit epoch, including participant decisions.
    pub applied_epoch: u64,
}

impl RecoveryWatermarks {
    pub(super) fn from_batch(batch: &XLBatch) -> Self {
        let mut water = Self::default();
        for entry in &batch.entries {
            water.max_xid = water.max_xid.max(entry.xid);
            water.applied_epoch = water.applied_epoch.max(entry.commit_epoch.unwrap_or(0));
            for op in &entry.ops {
                if let TxOp::ParticipantDecision(decision) = op {
                    if let TxDecision::Committed { commit_epoch } = decision.decision {
                        water.applied_epoch = water.applied_epoch.max(commit_epoch);
                    }
                }
            }
        }
        water
    }
}

impl WorkerStorage {
    // Only called after successful apply, while still holding the apply permit.
    pub(super) fn observe_recovery_watermarks(&self, water: RecoveryWatermarks) -> RS<()> {
        let mut cut = self.snapshot_cut.lock()?;
        cut.watermarks.max_xid = cut.watermarks.max_xid.max(water.max_xid);
        cut.watermarks.applied_epoch = cut.watermarks.applied_epoch.max(water.applied_epoch);
        Ok(())
    }
}
