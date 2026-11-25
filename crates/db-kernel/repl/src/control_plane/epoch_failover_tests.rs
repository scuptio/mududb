//! Epoch failover coverage: a voter failover keeps the epoch advancing, plus
//! the sparse-protocol (P4) variant of the same scenario. Shares the control
//! plane test harness in `tests.rs`.

use super::tests::{report_drained, Harness, TIMEOUT};
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn epoch_advances_survive_voter_failover() {
    epoch_advances_survive_voter_failover_impl(false);
}

/// The same failover scenario on the sparse epoch path (P4): advance in
/// memory within the committed reserve instead of committing per tick. The
/// sparse-specific difference: the in-memory advance is not replicated, so
/// the successor resumes from the committed state and re-advances once every
/// participant reports in the new term.
#[test]
fn epoch_advances_survive_voter_failover_sparse() {
    let _serial = super::test_guard();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let (harness, dir) = runtime.block_on(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("control-epoch-sparse-{}", mudu_sys::random::uuid_v4()));
        let harness = Harness::start_sparse(dir.clone(), &[1, 2, 3], &[], true)
            .await
            .unwrap();
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);

        // Drive one advance through the old leader; under sparse this moves
        // the shared state in memory within a committed reserve.
        let state = node.service.epoch_state().clone();
        let live = state.current.load(Ordering::Acquire);
        state.current.fetch_max(live + 4, Ordering::AcqRel);
        for participant in [1, 2, 3] {
            report_drained(&node.service, participant, live + 3);
        }
        mudu_sys::timeout(TIMEOUT, async {
            while state.stable.load(Ordering::Acquire) < live + 3 {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        let reached = state.stable.load(Ordering::Acquire);
        // The sparse ticker must have committed a reserve covering the
        // in-memory edge instead of per-tick advance records.
        let checkpoint = node.service.epoch_checkpoint().unwrap();
        assert!(
            checkpoint.reserved_upper >= reached,
            "sparse advance must stay inside the committed reserve: {checkpoint:?} vs reached {reached}"
        );

        // Fail over without a shutdown: the successor never saw the in-memory
        // advance, so its stable view cannot have overshot it.
        harness.isolate(leader);
        let successor = harness.wait_leader_excluding(&[leader]).await;
        let node = harness.node(successor);
        let state = node.service.epoch_state().clone();
        assert!(
            state.stable.load(Ordering::Acquire) <= reached,
            "the successor must not overshoot the old leader's in-memory edge"
        );
        // One report is not complete epoch evidence: no advance until every
        // participant reports in the new term.
        let live = state.current.load(Ordering::Acquire);
        state.current.fetch_max(live + 2, Ordering::AcqRel);
        report_drained(&node.service, successor, live + 1);
        mudu_sys::sleep(Duration::from_millis(100)).await.unwrap();
        let stalled = state.stable.load(Ordering::Acquire);
        // The failed owner rejoins; every participant reports, and the
        // successor advances past the reported drain.
        harness.rejoin(leader);
        for participant in [1, 2, 3] {
            report_drained(&node.service, participant, live + 1);
        }
        mudu_sys::timeout(TIMEOUT, async {
            while state.stable.load(Ordering::Acquire) <= stalled {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        (harness, dir)
    });
    harness.stop();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}

fn epoch_advances_survive_voter_failover_impl(sparse_epoch: bool) {
    let _serial = super::test_guard();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let (harness, dir) = runtime.block_on(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("control-epoch-{}", mudu_sys::random::uuid_v4()));
        let harness = Harness::start_sparse(dir.clone(), &[1, 2, 3], &[], sparse_epoch)
            .await
            .unwrap();
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);

        // Drive one advance through the old leader.
        let state = node.service.epoch_state().clone();
        let live = state.current.load(Ordering::Acquire);
        state.current.fetch_max(live + 4, Ordering::AcqRel);
        report_drained(&node.service, 1, live + 3);
        report_drained(&node.service, 2, live + 3);
        report_drained(&node.service, 3, live + 3);
        mudu_sys::timeout(TIMEOUT, async {
            while state.stable.load(Ordering::Acquire) < live + 3 {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        let reached = state.stable.load(Ordering::Acquire);

        // Fail over and confirm the successor keeps advancing the epoch from
        // at least the previously committed stable value.
        harness.isolate(leader);
        let successor = harness.wait_leader_excluding(&[leader]).await;
        let node = harness.node(successor);
        let state = node.service.epoch_state().clone();
        mudu_sys::timeout(TIMEOUT, async {
            while state.stable.load(Ordering::Acquire) < reached {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        let live = state.current.load(Ordering::Acquire);
        state.current.fetch_max(live + 2, Ordering::AcqRel);
        report_drained(&node.service, successor, live + 1);
        mudu_sys::sleep(Duration::from_millis(100)).await.unwrap();
        assert_eq!(state.stable.load(Ordering::Acquire), reached);
        // A quorum alone is not complete epoch evidence. The failed owner
        // must rejoin (or be explicitly fenced and taken over) before progress.
        harness.rejoin(leader);
        for participant in [1, 2, 3] {
            report_drained(&node.service, participant, live + 1);
        }
        mudu_sys::timeout(TIMEOUT, async {
            while state.stable.load(Ordering::Acquire) < live + 1 {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        (harness, dir)
    });
    harness.stop();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}
