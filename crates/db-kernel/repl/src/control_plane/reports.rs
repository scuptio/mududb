//! Complete, term-scoped reports for the transitional persisted epoch protocol.
use catalog::contract::epoch_op::EpochHeartbeat;
use std::collections::{BTreeSet, HashMap};

pub(super) struct EpochReports {
    participants: BTreeSet<u64>,
    term: Option<u64>,
    reports: HashMap<u64, u64>,
}

impl EpochReports {
    pub(super) fn new(participants: impl IntoIterator<Item = u64>) -> Self {
        Self {
            participants: participants.into_iter().collect(),
            term: None,
            reports: HashMap::new(),
        }
    }

    pub(super) fn note(&mut self, term: u64, report: EpochHeartbeat) {
        if report.control_term != term
            || !self.participants.contains(&report.node_id)
            // A fully drained node attests the live edge, which is exactly one
            // epoch above its closed prefix.
            || report.drained_epoch > report.closed_epoch.saturating_add(1)
        {
            return;
        }
        if self.term != Some(term) {
            self.reports.clear();
            self.term = Some(term);
        }
        self.reports
            .entry(report.node_id)
            .and_modify(|drained| *drained = (*drained).max(report.drained_epoch))
            .or_insert(report.drained_epoch);
    }

    pub(super) fn minimum(&self, term: u64) -> Option<u64> {
        if self.term != Some(term)
            || self.participants.is_empty()
            || self.reports.len() != self.participants.len()
        {
            return None;
        }
        self.reports.values().copied().min()
    }

    /// Fences a participant: removes it from the participant set and drops its
    /// report. Fencing is committed through the epoch stream (an `EpochOp::Fence`
    /// record) so it is durable and replicated; it is never driven by heartbeat
    /// timeout alone.
    pub(super) fn fence(&mut self, node_id: u64) {
        self.participants.remove(&node_id);
        self.reports.remove(&node_id);
    }

    /// Reconciles the participant set with the durable fenced set: every fenced
    /// node is removed, so the stable epoch may advance without its report.
    pub(super) fn reconcile(&mut self, fenced: &std::collections::BTreeSet<u64>) {
        for node in fenced.iter().copied() {
            self.fence(node);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(node_id: u64, control_term: u64, drained_epoch: u64) -> EpochHeartbeat {
        EpochHeartbeat {
            node_id,
            control_term,
            drained_epoch,
            closed_epoch: drained_epoch,
        }
    }
    #[test]
    fn missing_or_unknown_participant_cannot_complete_round() {
        let mut reports = EpochReports::new([1, 2, 3]);
        reports.note(7, report(1, 7, 10));
        reports.note(7, report(2, 7, 11));
        reports.note(7, report(4, 7, 20));
        assert_eq!(reports.minimum(7), None);
        reports.note(7, report(3, 7, 8));
        assert_eq!(reports.minimum(7), Some(8));
    }
    #[test]
    fn term_change_requires_every_participant_to_report_again() {
        let mut reports = EpochReports::new([1, 2]);
        reports.note(7, report(1, 7, 10));
        reports.note(7, report(2, 7, 10));
        assert_eq!(reports.minimum(8), None);
        reports.note(8, report(1, 8, 11));
        reports.note(8, report(2, 7, 100));
        assert_eq!(reports.minimum(8), None);
        reports.note(8, report(2, 8, 9));
        assert_eq!(reports.minimum(8), Some(9));
    }
    #[test]
    fn open_epochs_are_rejected_and_delayed_reports_do_not_regress() {
        let mut reports = EpochReports::new([1]);
        // A drained node may attest the live edge (closed + 1); anything beyond
        // it is rejected as an open epoch.
        let mut invalid = report(1, 7, 13);
        invalid.closed_epoch = 11;
        reports.note(7, invalid);
        assert_eq!(reports.minimum(7), None);
        reports.note(7, report(1, 7, 10));
        reports.note(7, report(1, 7, 9));
        assert_eq!(reports.minimum(7), Some(10));
    }

    #[test]
    fn fencing_removes_a_participant_and_unblocks_the_advance() {
        let mut reports = EpochReports::new([1, 2, 3]);
        reports.note(7, report(1, 7, 10));
        reports.note(7, report(2, 7, 11));
        // Node 3 has not reported: the round is incomplete.
        assert_eq!(reports.minimum(7), None);
        // Fencing node 3 (durably, not by timeout) drops it and its missing
        // report, so the minimum is now well-defined.
        reports.fence(3);
        assert_eq!(reports.minimum(7), Some(10));
        // Re-fencing is idempotent.
        reports.fence(3);
        assert_eq!(reports.minimum(7), Some(10));
    }

    #[test]
    fn reconcile_drops_the_durable_fenced_set() {
        let mut reports = EpochReports::new([1, 2, 3]);
        reports.note(7, report(1, 7, 10));
        reports.note(7, report(2, 7, 11));
        reports.note(7, report(3, 7, 9));
        assert_eq!(reports.minimum(7), Some(9));
        let fenced = std::collections::BTreeSet::from([2u64, 3]);
        reports.reconcile(&fenced);
        assert_eq!(reports.minimum(7), Some(10));
    }
}
