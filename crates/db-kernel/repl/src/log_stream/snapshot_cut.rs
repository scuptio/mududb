//! Exclusive business-apply cut used while capturing group snapshots.
use super::state::invalid;
use super::{GroupState, StreamApplyScheduler};
use mudu::common::result::RS;
use tokio::sync::OwnedMutexGuard;

/// All local stream apply owners held at one exact committed vector.
/// Keep this guard alive until every business image has been captured.
/// This is not a schema/placement barrier, a Raft membership certificate,
/// or permission to compact any log. Mutations outside the scheduler require
/// their own storage barriers before a group snapshot may be published.
#[must_use = "dropping the cut permits stream apply to resume"]
pub struct AppliedGroupCut {
    state: GroupState,
    _owners: Vec<OwnedMutexGuard<()>>,
}

impl AppliedGroupCut {
    /// Complete registry and committed vector verified under the apply locks.
    pub fn state(&self) -> &GroupState {
        &self.state
    }
}

impl StreamApplyScheduler {
    /// Freezes every live adapter and requires exact applied/durable agreement
    /// with the supplied authoritative vector. Does not advance lagging streams
    /// while holding the locks: the caller must release, catch up and retry.
    /// Acquires locks in stream-id order, making simultaneous captures safe.
    /// Cancellation releases all acquired locks without changing business state.
    pub async fn capture_applied_cut(&self, state: &GroupState) -> RS<AppliedGroupCut> {
        if state.identity() != self.identity {
            return Err(invalid("foreign snapshot apply vector"));
        }
        Self::new(state, self.adapters.values().cloned().collect())?;
        let mut owners = Vec::with_capacity(self.owners.len());
        for owner in self.owners.values() {
            owners.push(owner.clone().lock_owned().await);
        }
        for (id, adapter) in self.adapters.iter() {
            let applied = adapter.applied_frontier().await?;
            if applied.as_ref() != state.committed().get(id) {
                return Err(invalid(
                    "snapshot requires exact applied and committed vectors",
                ));
            }
            if let Some(prefix) = applied {
                if !adapter.contains_applied(&prefix).await? {
                    return Err(invalid("snapshot applied prefix lacks durable history"));
                }
            }
        }
        Ok(AppliedGroupCut {
            state: state.clone(),
            _owners: owners,
        })
    }
}
