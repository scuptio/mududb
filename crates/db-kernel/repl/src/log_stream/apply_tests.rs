//! Apply ordering, dependency and failure isolation regressions.
#![allow(clippy::unwrap_used)]
use super::tests::{apply, change, commit, descriptor, identity, progress};
use super::*;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_raft::stream::*;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;

struct Counter {
    descriptor: StreamDescriptor,
    records: Vec<StreamStruct>,
    applied: Mutex<Vec<StreamProgress>>,
    missing: bool,
    history_missing: bool,
    stalled: Option<Arc<tokio::sync::Notify>>,
}
#[async_trait]
impl StreamAdapter for Counter {
    fn descriptor(&self) -> &StreamDescriptor {
        &self.descriptor
    }
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(self.applied.lock().await.last().cloned())
    }
    async fn next_committed(
        &self,
        after: Option<&StreamProgress>,
        _: &StreamProgress,
    ) -> RS<Option<StreamStruct>> {
        if let Some(stalled) = &self.stalled {
            stalled.notified().await;
        }
        if self.missing {
            return Err(super::state::invalid("missing durable segment"));
        }
        Ok(self
            .records
            .get(after.map_or(0, |p| p.end_lsn as usize + 1))
            .cloned())
    }
    async fn apply_committed(&self, record: StreamStruct) -> RS<()> {
        self.applied.lock().await.push(record.progress);
        Ok(())
    }
    async fn contains_applied(&self, prefix: &StreamProgress) -> RS<bool> {
        Ok(!self.history_missing && self.applied.lock().await.contains(prefix))
    }
}
fn counter(
    id: u64,
    requires: Vec<StreamId>,
    deps: Vec<StreamProgress>,
    missing: bool,
) -> Arc<Counter> {
    let mut descriptor = descriptor(id);
    descriptor.requires = requires;
    Arc::new(Counter {
        descriptor,
        records: vec![StreamStruct {
            identity: identity(),
            progress: progress(id, 0),
            dependencies: deps,
            payload: vec![1],
        }],
        applied: Mutex::new(Vec::new()),
        missing,
        history_missing: false,
        stalled: None,
    })
}
fn state(counters: &[Arc<Counter>]) -> GroupState {
    let mut state = GroupState::new(identity()).unwrap();
    for (i, counter) in counters.iter().enumerate() {
        let entry = change(
            &state,
            i as u128 + 1,
            RegistryOp::Register(counter.descriptor.clone()),
        );
        apply(&mut state, &entry).unwrap();
    }
    let entry = commit(
        &state,
        counters
            .iter()
            .map(|c| progress(c.descriptor.stream_id.0, 0))
            .collect(),
    );
    apply(&mut state, &entry).unwrap();
    state
}
fn scheduler(state: &GroupState, counters: &[Arc<Counter>]) -> StreamApplyScheduler {
    StreamApplyScheduler::new(
        state,
        counters
            .iter()
            .cloned()
            .map(|c| c as Arc<dyn StreamAdapter>)
            .collect(),
    )
    .unwrap()
}

/// Runs a scheduler round inside a LocalSet: per-stream apply sub-tasks are
/// `spawn_local`, matching the production executors (business apply may spawn
/// its own thread-confined tasks).
async fn round(
    scheduler: &StreamApplyScheduler,
    state: &GroupState,
) -> RS<BTreeMap<StreamId, RS<ApplyOutcome>>> {
    tokio::task::LocalSet::new()
        .run_until(scheduler.apply_round(state))
        .await
}
#[tokio::test]
async fn missing_schema_blocks_dependents_but_not_independent_streams() {
    let counters = vec![
        counter(1, vec![], vec![], true),
        counter(2, vec![StreamId(1)], vec![progress(1, 0)], false),
        counter(3, vec![], vec![], false),
    ];
    let state = state(&counters);
    let scheduler = scheduler(&state, &counters);
    let results = round(&scheduler, &state).await.unwrap();
    assert!(results[&StreamId(1)].is_err());
    assert_eq!(
        results[&StreamId(2)].as_ref().unwrap(),
        &ApplyOutcome::Waiting
    );
    assert_eq!(
        results[&StreamId(3)].as_ref().unwrap(),
        &ApplyOutcome::Applied(progress(3, 0))
    );
    assert!(counters[1].applied.lock().await.is_empty());
    assert_eq!(
        round(&scheduler, &state).await.unwrap()[&StreamId(3)]
            .as_ref()
            .unwrap(),
        &ApplyOutcome::Current
    );
}
#[tokio::test]
async fn dependency_apply_and_repeated_rounds_preserve_exactly_once_order() {
    let counters = vec![
        counter(1, vec![], vec![], false),
        counter(2, vec![StreamId(1)], vec![progress(1, 0)], false),
    ];
    let state = state(&counters);
    let scheduler = scheduler(&state, &counters);
    for _ in 0..3 {
        round(&scheduler, &state).await.unwrap();
    }
    for counter in &counters {
        assert_eq!(counter.applied.lock().await.len(), 1);
    }
}
#[tokio::test]
async fn malformed_records_never_reach_business_apply() {
    for case in 0..7 {
        let mut target = counter(1, vec![], vec![], false);
        let record = &mut Arc::get_mut(&mut target).unwrap().records[0];
        match case {
            0 => record.identity.generation += 1,
            1 => record.progress.stream_generation += 1,
            2 => record.progress.end_lsn += 1,
            3 => record.progress.segment_digest[0] += 1,
            4 => record.payload = vec![0; 1025],
            5 => record.dependencies.push(progress(2, 0)),
            _ => record.progress.stream_id = StreamId(2),
        }
        let counters = vec![target];
        let state = state(&counters);
        assert!(round(&scheduler(&state, &counters), &state).await.unwrap()[&StreamId(1)].is_err());
        assert!(counters[0].applied.lock().await.is_empty());
    }
}
#[tokio::test]
async fn recovery_rejects_missing_adapters_and_speculative_applied_frontiers() {
    let counters = vec![counter(1, vec![], vec![], false)];
    let state = state(&counters);
    assert!(StreamApplyScheduler::new(&state, vec![]).is_err());
    counters[0].applied.lock().await.push(progress(1, 1));
    assert!(round(&scheduler(&state, &counters), &state).await.unwrap()[&StreamId(1)].is_err());
}

#[tokio::test]
async fn dependencies_require_exact_historical_digest_and_committed_authority() {
    for case in 0..3 {
        let mut dependency = progress(1, 0);
        if case == 0 {
            dependency.segment_digest[0] = 99;
        }
        if case == 1 {
            dependency.end_lsn = 2;
        }
        if case == 2 {
            dependency.stream_generation = 2;
        }
        let counters = vec![
            counter(1, vec![], vec![], false),
            counter(2, vec![StreamId(1)], vec![dependency], false),
        ];
        let mut state = state(&counters);
        // The source has advanced past the requested dependency. Its current
        // LSN alone cannot prove the requested historical digest was applied.
        state.committed.insert(StreamId(1), progress(1, 1));
        counters[0]
            .applied
            .lock()
            .await
            .extend([progress(1, 0), progress(1, 1)]);
        let result = round(&scheduler(&state, &counters), &state).await.unwrap();
        if case == 2 {
            assert!(result[&StreamId(2)].is_err());
        } else {
            assert_eq!(
                result[&StreamId(2)].as_ref().unwrap(),
                &ApplyOutcome::Waiting
            );
        }
        assert!(counters[1].applied.lock().await.is_empty());
    }
}

#[tokio::test]
async fn stalled_adapter_does_not_hold_an_independent_stream_owner() {
    let release = Arc::new(tokio::sync::Notify::new());
    let mut slow = counter(1, vec![], vec![], false);
    Arc::get_mut(&mut slow).unwrap().stalled = Some(release.clone());
    let counters = vec![slow, counter(2, vec![], vec![], false)];
    let state = state(&counters);
    let scheduler = scheduler(&state, &counters);
    let mut pending = Box::pin(scheduler.apply_stream(&state, StreamId(1)));
    assert!(
        mudu_sys::timeout(std::time::Duration::from_millis(5), &mut pending)
            .await
            .is_none()
    );
    let result = mudu_sys::timeout(
        std::time::Duration::from_secs(1),
        scheduler.apply_stream(&state, StreamId(2)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, ApplyOutcome::Applied(progress(2, 0)));
    release.notify_one();
    assert_eq!(
        pending.await.unwrap(),
        ApplyOutcome::Applied(progress(1, 0))
    );
}

#[tokio::test]
async fn snapshot_cut_requires_exact_vector_and_releases_after_rejection() {
    let counters = vec![
        counter(1, vec![], vec![], false),
        counter(2, vec![], vec![], false),
    ];
    let state = state(&counters);
    let scheduler = scheduler(&state, &counters);
    assert!(scheduler.capture_applied_cut(&state).await.is_err());
    round(&scheduler, &state).await.unwrap();
    let cut = scheduler.capture_applied_cut(&state).await.unwrap();
    assert_eq!(cut.state().committed(), state.committed());
    drop(cut);
    let mut older = state.clone();
    older.committed.remove(&StreamId(2));
    assert!(scheduler.capture_applied_cut(&older).await.is_err());
    let mut wrong = state.clone();
    wrong
        .committed
        .get_mut(&StreamId(1))
        .unwrap()
        .segment_digest[0] ^= 1;
    assert!(scheduler.capture_applied_cut(&wrong).await.is_err());
    assert!(scheduler.capture_applied_cut(&state).await.is_ok());
}

#[tokio::test]
async fn snapshot_cut_blocks_every_stream_and_releases_on_drop() {
    let counters = vec![
        counter(1, vec![], vec![], false),
        counter(2, vec![], vec![], false),
    ];
    let state = state(&counters);
    let scheduler = scheduler(&state, &counters);
    round(&scheduler, &state).await.unwrap();
    let cut = scheduler.capture_applied_cut(&state).await.unwrap();
    for id in [StreamId(1), StreamId(2)] {
        assert!(mudu_sys::timeout(
            std::time::Duration::from_millis(10),
            scheduler.apply_stream(&state, id)
        )
        .await
        .is_none());
    }
    // A competing capture may be cancelled without releasing the first cut.
    assert!(mudu_sys::timeout(
        std::time::Duration::from_millis(10),
        scheduler.capture_applied_cut(&state)
    )
    .await
    .is_none());
    drop(cut);
    for id in [StreamId(1), StreamId(2)] {
        assert_eq!(
            scheduler.apply_stream(&state, id).await.unwrap(),
            ApplyOutcome::Current
        );
    }
    assert!(scheduler.capture_applied_cut(&state).await.is_ok());
}

#[tokio::test]
async fn snapshot_cut_rejects_missing_history_and_foreign_namespace() {
    let mut counter = counter(1, vec![], vec![], false);
    Arc::get_mut(&mut counter).unwrap().history_missing = true;
    let counters = vec![counter];
    let state = state(&counters);
    let scheduler = scheduler(&state, &counters);
    round(&scheduler, &state).await.unwrap();
    assert!(scheduler.capture_applied_cut(&state).await.is_err());
    let mut foreign = state.clone();
    foreign.identity.generation += 1;
    assert!(scheduler.capture_applied_cut(&foreign).await.is_err());
    // Rejection does not leave the apply owner locked.
    assert_eq!(
        scheduler.apply_stream(&state, StreamId(1)).await.unwrap(),
        ApplyOutcome::Current
    );
}
