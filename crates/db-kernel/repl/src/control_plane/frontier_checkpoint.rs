//! The epoch stream's business checkpoint, carried in its applied frontier:
//! the last applied advance, so a restarted node resumes its shared epoch
//! state instead of regressing to the epoch base.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The business checkpoint the epoch stream carries in its frontier:
/// the last applied advance, so a restarted node resumes its shared epoch
/// state instead of regressing to the epoch base. It also carries the fenced
/// data-plane nodes so a restarted replica rebuilds the epoch participant set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochCheckpoint {
    /// The cluster stable epoch at the frontier.
    pub stable: u64,
    /// The collecting epoch at the frontier.
    pub current: u64,
    /// The inclusive upper bound of reserved epoch numbers; in-memory advance
    /// must not hand out an epoch at or above this bound (E-02).
    pub reserved_upper: u64,
    /// The physical time of the advance (epoch → wall-clock mapping).
    pub physical_millis: u64,
    /// Data-plane nodes fenced so far: they no longer provide drain evidence
    /// and are excluded from the epoch participant set. Monotonic (only grows).
    pub fenced: BTreeSet<u64>,
}

/// The initial number of epochs reserved ahead of the base, so a fresh leader
/// can advance in memory before its first committed reserve.
const INITIAL_RESERVE: u64 = 1_000_000;

impl EpochCheckpoint {
    /// The initial checkpoint of a fresh epoch stream (base epoch, one live
    /// edge, a starter reserve). Public so the data-plane control client can
    /// replay committed epoch records independently of the control Raft group.
    pub fn initial() -> Self {
        let base = catalog::contract::epoch_oracle::EPOCH_XID_BASE;
        Self {
            stable: base,
            current: base + 1,
            reserved_upper: base + INITIAL_RESERVE,
            physical_millis: 0,
            fenced: BTreeSet::new(),
        }
    }

    /// Applies one committed epoch op to the checkpoint (monotonic advance,
    /// reserve extension or idempotent fence). Public so the data-plane
    /// control client replays committed epoch records off the control group.
    pub fn advance(
        self,
        op: catalog::contract::epoch_op::EpochOp,
    ) -> mudu::common::result::RS<Self> {
        match op {
            catalog::contract::epoch_op::EpochOp::Advance {
                stable,
                current,
                physical_millis,
            } => {
                let stable = stable.max(self.stable);
                let next = stable.checked_add(1).ok_or_else(|| {
                    mudu::mudu_error!(
                        mudu::error::ErrorCode::InvalidState,
                        "epoch space exhausted"
                    )
                })?;
                Ok(Self {
                    stable,
                    current: current.max(next).max(self.current),
                    physical_millis,
                    ..self
                })
            }
            catalog::contract::epoch_op::EpochOp::Reserve { reserved_upper } => {
                // The reserve is monotonic; a lower bound never shrinks it.
                Ok(Self {
                    reserved_upper: reserved_upper.max(self.reserved_upper),
                    ..self
                })
            }
            catalog::contract::epoch_op::EpochOp::Fence { node_id } => {
                // Idempotent: re-fencing an already-fenced node is a no-op.
                let mut fenced = self.fenced;
                fenced.insert(node_id);
                Ok(Self { fenced, ..self })
            }
        }
    }

    /// Whether the live edge has exhausted the reserved range: the in-memory
    /// advance must not hand out an epoch at or above `reserved_upper`.
    #[allow(dead_code)]
    pub(super) fn exhausted(&self) -> bool {
        self.current >= self.reserved_upper
    }

    /// Whether the live edge is within `margin` of the reserved upper, so the
    /// leader should commit a fresh reserve before handing out more epochs.
    pub(super) fn reserve_needed(&self, margin: u64) -> bool {
        self.current.saturating_add(margin) >= self.reserved_upper
    }

    /// Whether `self` monotonically extends `committed` (the replay of the
    /// committed epoch history). The sparse in-memory advance may move the
    /// checkpoint ahead of the committed log, so recovery accepts a checkpoint
    /// that covers the replay rather than requiring byte equality.
    pub(super) fn covers(&self, committed: &EpochCheckpoint) -> bool {
        self.stable >= committed.stable
            && self.current >= committed.current
            && self.reserved_upper >= committed.reserved_upper
            && self.physical_millis >= committed.physical_millis
            && self.fenced.is_superset(&committed.fenced)
    }

    /// Advances the in-memory `stable`/`current` within the reserved range,
    /// without a committed control record (E-04/E-06). Fails closed when the
    /// live edge would reach or cross the reserved upper: the leader must
    /// commit a fresh reserve before handing out more epochs.
    pub(super) fn advance_in_memory(
        self,
        stable: u64,
        current: u64,
        physical_millis: u64,
    ) -> mudu::common::result::RS<Self> {
        let stable = stable.max(self.stable);
        let next = stable.checked_add(1).ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidState,
                "epoch space exhausted"
            )
        })?;
        let current = current.max(next).max(self.current);
        if current >= self.reserved_upper {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidState,
                "epoch advance crosses the reserved upper"
            ));
        }
        Ok(Self {
            stable,
            current,
            physical_millis,
            ..self
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use catalog::contract::epoch_op::EpochOp;

    #[test]
    fn reserve_extends_the_upper_bound_monotonically() {
        let base = catalog::contract::epoch_oracle::EPOCH_XID_BASE;
        let initial = EpochCheckpoint::initial();
        assert_eq!(initial.reserved_upper, base + INITIAL_RESERVE);

        let reserved = initial
            .clone()
            .advance(EpochOp::Reserve {
                reserved_upper: base + 2 * INITIAL_RESERVE,
            })
            .unwrap();
        assert_eq!(reserved.reserved_upper, base + 2 * INITIAL_RESERVE);
        assert_eq!(reserved.stable, initial.stable);
        assert_eq!(reserved.current, initial.current);

        // A lower reserve never shrinks the bound.
        let lower = reserved
            .clone()
            .advance(EpochOp::Reserve {
                reserved_upper: base + INITIAL_RESERVE,
            })
            .unwrap();
        assert_eq!(lower.reserved_upper, base + 2 * INITIAL_RESERVE);

        // An advance keeps the reserved bound.
        let advanced = reserved
            .clone()
            .advance(EpochOp::Advance {
                stable: base,
                current: base + 2,
                physical_millis: 1,
            })
            .unwrap();
        assert_eq!(advanced.reserved_upper, base + 2 * INITIAL_RESERVE);
        assert_eq!(advanced.stable, base);
    }

    #[test]
    fn in_memory_advance_is_bounded_by_the_reserve_and_replenishes() {
        // A checkpoint with a tiny reserve to exercise exhaustion.
        let base = catalog::contract::epoch_oracle::EPOCH_XID_BASE;
        let checkpoint = EpochCheckpoint {
            stable: base,
            current: base + 1,
            reserved_upper: base + 4,
            physical_millis: 0,
            fenced: BTreeSet::new(),
        };
        assert!(!checkpoint.exhausted());
        // Advance within the range.
        let advanced = checkpoint.advance_in_memory(base, base + 2, 1).unwrap();
        assert_eq!(advanced.stable, base);
        assert_eq!(advanced.current, base + 2);
        // Near the upper: reserve is needed.
        assert!(advanced.reserve_needed(2));
        assert!(!advanced.exhausted());
        // Crossing the upper fails closed.
        assert!(advanced
            .clone()
            .advance_in_memory(base, base + 4, 2)
            .is_err());
        // A fresh reserve unlocks further in-memory advance.
        let reserved = advanced
            .clone()
            .advance(EpochOp::Reserve {
                reserved_upper: base + 100,
            })
            .unwrap();
        assert!(!reserved.reserve_needed(1));
        let next = reserved.advance_in_memory(base, base + 4, 3).unwrap();
        assert_eq!(next.current, base + 4);
    }

    #[test]
    fn fence_is_idempotent_and_monotonic() {
        let checkpoint = EpochCheckpoint::initial();
        assert!(checkpoint.fenced.is_empty());

        let fenced = checkpoint.advance(EpochOp::Fence { node_id: 7 }).unwrap();
        assert_eq!(fenced.fenced.iter().copied().collect::<Vec<_>>(), vec![7]);
        // The epoch values are unchanged by a fence.
        assert_eq!(fenced.stable, EpochCheckpoint::initial().stable);
        assert_eq!(fenced.current, EpochCheckpoint::initial().current);

        // Re-fencing the same node is a no-op; a second node is added.
        let again = fenced
            .clone()
            .advance(EpochOp::Fence { node_id: 7 })
            .unwrap();
        assert_eq!(again.fenced, fenced.fenced);
        let both = fenced
            .clone()
            .advance(EpochOp::Fence { node_id: 9 })
            .unwrap();
        assert_eq!(both.fenced.iter().copied().collect::<Vec<_>>(), vec![7, 9]);

        // A checkpoint with fenced nodes covers one without.
        assert!(both.covers(&fenced));
        assert!(!fenced.covers(&both));
    }
}
