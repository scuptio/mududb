//! Bounded concurrent apply rounds over an authoritative committed vector.

use super::state::invalid;
use super::{GroupState, StreamAdapter, StreamLifecycle, StreamStruct};
use mudu::common::result::RS;
use mudu_raft::stream::{StreamId, StreamProgress};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

/// One stream's outcome; a failed or waiting stream does not prevent other
/// streams from applying. Call again after dependencies or missing data arrive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Already at the authoritative frontier.
    Current,
    /// Required business prefixes have not yet been applied.
    Waiting,
    /// Applied one record; work per stream per round is bounded.
    Applied(StreamProgress),
}

/// Applies at most one record per stream concurrently. A per-stream lock
/// prevents concurrent callers from applying the same stream out of order.
/// This is the apply component, not a leader admission or snapshot barrier.
pub struct StreamApplyScheduler {
    pub(super) adapters: Arc<BTreeMap<StreamId, Arc<dyn StreamAdapter>>>,
    pub(super) identity: mudu_raft::stream::GroupIdobject,
    pub(super) owners: BTreeMap<StreamId, Arc<Mutex<()>>>,
    partial: bool,
}

impl StreamApplyScheduler {
    /// Binds exactly the live registry to local adapters; unknown formats,
    /// missing adapters and extra identities fail recovery closed.
    pub fn new(state: &GroupState, adapters: Vec<Arc<dyn StreamAdapter>>) -> RS<Self> {
        Self::build(state, adapters, false)
    }

    /// Binds a subset of the live registry. On a shared group each plane's
    /// apply driver owns only its streams, so the scheduler may omit the other
    /// plane's streams; the combined assembly is responsible for covering every
    /// live stream across its schedulers.
    pub fn new_partial(state: &GroupState, adapters: Vec<Arc<dyn StreamAdapter>>) -> RS<Self> {
        Self::build(state, adapters, true)
    }

    fn build(state: &GroupState, adapters: Vec<Arc<dyn StreamAdapter>>, partial: bool) -> RS<Self> {
        let mut bound = BTreeMap::new();
        for adapter in adapters {
            let id = adapter.descriptor().stream_id;
            let stream = state
                .streams()
                .get(&id)
                .ok_or_else(|| invalid("unknown adapter"))?;
            if stream.lifecycle == StreamLifecycle::Tombstoned
                || stream.descriptor != *adapter.descriptor()
                || bound.insert(id, adapter).is_some()
            {
                return Err(invalid("duplicate, retired or incompatible adapter"));
            }
        }
        if !partial
            && state
                .streams()
                .values()
                .filter(|s| s.lifecycle != StreamLifecycle::Tombstoned)
                .count()
                != bound.len()
        {
            return Err(invalid("missing live stream adapter"));
        }
        let owners = bound
            .keys()
            .map(|id| (*id, Arc::new(Mutex::new(()))))
            .collect();
        Ok(Self {
            adapters: Arc::new(bound),
            identity: state.identity(),
            owners,
            partial,
        })
    }

    /// Advances one stream independently of stalled or slow unrelated streams.
    /// Concurrent calls for the same stream serialize through its own owner.
    pub async fn apply_stream(&self, state: &GroupState, id: StreamId) -> RS<ApplyOutcome> {
        if state.identity() != self.identity {
            return Err(invalid("foreign apply vector"));
        }
        Self::build(
            state,
            self.adapters.values().cloned().collect(),
            self.partial,
        )?;
        let owner = self
            .owners
            .get(&id)
            .ok_or_else(|| invalid("unknown apply stream"))?;
        let _guard = owner.lock().await;
        let committed = state
            .committed()
            .get(&id)
            .ok_or_else(|| invalid("stream has no committed prefix"))?;
        apply_one(
            state.identity(),
            self.adapters[&id].clone(),
            committed.clone(),
            &self.adapters,
            state.committed(),
        )
        .await
    }

    /// Runs a bounded round. The caller supplies a locally replayed authoritative
    /// view, never a speculative proposal. Results are separated by stream.
    /// Higher-priority streams' tasks are spawned first so a shared executor
    /// starts them before bulk streams under load. Dropping this future
    /// aborts tasks; adapters must honor cancellation safety.
    pub async fn apply_round(
        &self,
        state: &GroupState,
    ) -> RS<BTreeMap<StreamId, RS<ApplyOutcome>>> {
        if state.identity() != self.identity {
            return Err(invalid("foreign apply vector"));
        }
        // Revalidate registration before any business effect.
        Self::build(
            state,
            self.adapters.values().cloned().collect(),
            self.partial,
        )?;
        let mut tasks = JoinSet::new();
        let vector = Arc::new(state.committed().clone());
        // Higher scheduling priority first: the epoch quota renewal must not
        // queue behind a bulk catalog stream.
        let mut ordered: Vec<_> = state.committed().iter().collect();
        ordered.sort_by_key(|(id, _)| {
            std::cmp::Reverse(
                state
                    .streams()
                    .get(id)
                    .map(|stream| stream.descriptor.budget.priority)
                    .unwrap_or(0),
            )
        });
        for (&id, committed) in ordered {
            // Streams outside this scheduler's adapter set belong to another
            // plane's apply driver on a shared group; skip them here.
            let Some(adapter) = self.adapters.get(&id) else {
                continue;
            };
            let adapter = adapter.clone();
            let adapters = self.adapters.clone();
            let identity = state.identity();
            let committed = committed.clone();
            let owner = self.owners[&id].clone();
            let vector = vector.clone();
            // Local-set spawn: business apply reaches storage helpers that
            // spawn their own thread-confined tasks (e.g. the paged index
            // handle), so the apply sub-task must keep the LocalSet context.
            tasks.spawn_local(async move {
                let _guard = owner.lock_owned().await;
                let result = apply_one(identity, adapter, committed, &adapters, &vector).await;
                (id, result)
            });
        }
        let mut outcomes = BTreeMap::new();
        while let Some(result) = tasks.join_next().await {
            let (id, result) = result.map_err(|_| invalid("stream apply task failed"))?;
            outcomes.insert(id, result);
        }
        Ok(outcomes)
    }
}

async fn apply_one(
    identity: mudu_raft::stream::GroupIdobject,
    adapter: Arc<dyn StreamAdapter>,
    committed: StreamProgress,
    adapters: &BTreeMap<StreamId, Arc<dyn StreamAdapter>>,
    vector: &BTreeMap<StreamId, StreamProgress>,
) -> RS<ApplyOutcome> {
    let applied = adapter.applied_frontier().await?;
    if let Some(prefix) = &applied {
        if prefix.stream_id != committed.stream_id
            || prefix.stream_generation != committed.stream_generation
            || prefix.end_lsn > committed.end_lsn
        {
            return Err(invalid(
                "applied frontier exceeds or mismatches committed prefix",
            ));
        }
        if prefix.end_lsn == committed.end_lsn {
            return if *prefix == committed {
                Ok(ApplyOutcome::Current)
            } else {
                Err(invalid("applied digest disagrees with consensus"))
            };
        }
    }
    // Pin the log read floor across the next-committed read and the apply, so
    // prefix GC cannot reclaim the records this round is about to consume.
    let floor = applied.as_ref().map_or(0, |p| p.end_lsn.saturating_add(1));
    let _read_pin = adapter.pin_read(floor);
    let record = adapter
        .next_committed(applied.as_ref(), &committed)
        .await?
        .ok_or_else(|| invalid("missing next committed record"))?;
    validate_record(
        identity,
        adapter.descriptor(),
        applied.as_ref(),
        &committed,
        &record,
    )?;
    for dependency in &record.dependencies {
        let Some(frontier) = vector.get(&dependency.stream_id) else {
            return Ok(ApplyOutcome::Waiting);
        };
        if dependency.stream_generation != frontier.stream_generation {
            return Err(invalid("dependency references a stale incarnation"));
        }
        if dependency.end_lsn > frontier.end_lsn {
            return Ok(ApplyOutcome::Waiting);
        }
        if dependency.end_lsn == frontier.end_lsn && dependency != frontier {
            return Err(invalid("dependency digest disagrees with consensus"));
        }
        let source = adapters
            .get(&dependency.stream_id)
            .ok_or_else(|| invalid("missing dependency adapter"))?;
        if source.descriptor().stream_generation != dependency.stream_generation {
            return Err(invalid("dependency generation mismatch"));
        }
        if !source.contains_applied(dependency).await? {
            return Ok(ApplyOutcome::Waiting);
        }
    }
    let progress = record.progress.clone();
    adapter.apply_committed(record).await?;
    if adapter.applied_frontier().await? != Some(progress.clone()) {
        return Err(invalid("adapter failed to persist applied frontier"));
    }
    Ok(ApplyOutcome::Applied(progress))
}

fn validate_record(
    identity: mudu_raft::stream::GroupIdobject,
    descriptor: &mudu_raft::stream::StreamDescriptor,
    applied: Option<&StreamProgress>,
    committed: &StreamProgress,
    record: &StreamStruct,
) -> RS<()> {
    let next = match applied {
        Some(p) => p.end_lsn.checked_add(1),
        None => Some(0),
    };
    if record.identity != identity
        || record.progress.stream_id != committed.stream_id
        || record.progress.stream_generation != committed.stream_generation
        || Some(record.progress.end_lsn) != next
        || record.progress.end_lsn > committed.end_lsn
        || (record.progress.end_lsn == committed.end_lsn && record.progress != *committed)
        || record.payload.len() > descriptor.budget.max_record_bytes as usize
        || record.dependencies.len() > descriptor.requires.len()
    {
        return Err(invalid(
            "invalid committed record identity, sequence, digest or budget",
        ));
    }
    let mut seen = BTreeSet::new();
    for dependency in &record.dependencies {
        if !descriptor.requires.contains(&dependency.stream_id)
            || !seen.insert(dependency.stream_id)
        {
            return Err(invalid("undeclared or duplicate record dependency"));
        }
    }
    Ok(())
}
