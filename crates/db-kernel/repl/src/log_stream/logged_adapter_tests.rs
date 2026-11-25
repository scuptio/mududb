//! Custom business adapter over a real durable external log and one shared Raft.
#![allow(clippy::unwrap_used)]
use super::tests::{adapters, descriptor, identity, wait_leader};
use super::*;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_raft::stream::{GroupId, GroupIdobject, RegistryOp, StreamId, StreamProgress};
use mudu_raft::{InMemoryNetwork, RaftCfg, RaftHandle};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default, Serialize, Deserialize)]
struct Image {
    value: u64,
    frontier: Option<StreamProgress>,
    results: BTreeMap<u128, u64>,
}
struct Counter {
    path: PathBuf,
    owner: Mutex<()>,
}
impl Counter {
    async fn load(&self) -> Image {
        let fs = mudu_sys::fs::tokio_fs();
        if fs.path_exists(&self.path).await.unwrap() {
            rmp_serde::from_slice(&fs.read_all(&self.path).await.unwrap()).unwrap()
        } else {
            Image::default()
        }
    }
}
#[async_trait]
impl StreamStateMachine for Counter {
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        let _owner = self.owner.lock().await;
        Ok(self.load().await.frontier)
    }
    async fn apply_once(&self, request_id: u128, record: StreamStruct) -> RS<()> {
        let _owner = self.owner.lock().await;
        // Reload the atomically published state, including after cancellation.
        let mut image = self.load().await;
        if image.results.contains_key(&request_id) {
            return Ok(());
        }
        image.value += record.payload[0] as u64;
        image.results.insert(request_id, image.value);
        image.frontier = Some(record.progress);
        crate::durable_io::publish(
            mudu_sys::fs::tokio_fs().as_ref(),
            &self.path,
            &rmp_serde::to_vec(&image).unwrap(),
        )
        .await
    }
    async fn snapshot_at(&self, prefix: Option<&StreamProgress>) -> RS<Vec<u8>> {
        let _owner = self.owner.lock().await;
        let image = self.load().await;
        if image.frontier.as_ref() != prefix {
            return Err(super::state::invalid("counter snapshot prefix mismatch"));
        }
        rmp_serde::to_vec(&image)
            .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Encode, "encode counter", e))
    }
    async fn restore_at(&self, prefix: Option<&StreamProgress>, business: &[u8]) -> RS<()> {
        let _owner = self.owner.lock().await;
        let image: Image = rmp_serde::from_slice(business)
            .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Decode, "decode counter", e))?;
        if image.frontier.as_ref() != prefix {
            return Err(super::state::invalid("counter restore prefix mismatch"));
        }
        let existing = self.load().await;
        if existing.frontier.is_some() {
            if existing.frontier != image.frontier {
                return Err(super::state::invalid(
                    "counter restore would overwrite a different frontier",
                ));
            }
            return Ok(());
        }
        crate::durable_io::publish(mudu_sys::fs::tokio_fs().as_ref(), &self.path, business).await
    }
}
struct NoPeers;
#[async_trait]
impl StreamTransport for NoPeers {
    async fn session(&self, _: u64) -> RS<StreamSession> {
        Err(super::state::invalid("unexpected peer"))
    }
    async fn push(
        &self,
        _: u64,
        _: &StreamSession,
        _: &AckRound,
        _: Vec<u8>,
    ) -> RS<Option<DurableAck>> {
        Err(super::state::invalid("unexpected peer"))
    }
    async fn fetch(&self, _: u64, _: &StreamSession, _: StreamFetch) -> RS<Vec<u8>> {
        Err(super::state::invalid("unexpected peer"))
    }
    async fn progress_at(
        &self,
        _: u64,
        _: &StreamSession,
        _: StreamId,
        _: u64,
    ) -> RS<Option<StreamProgress>> {
        Err(super::state::invalid("unexpected peer"))
    }
}
struct Fixture {
    group: GenericReplicationGroup,
    registry: Arc<ReplicatedStreamRegistry>,
    scheduler: StreamApplyScheduler,
    counters: Vec<Arc<Counter>>,
}
async fn open(path: &Path) -> Fixture {
    let raft = Arc::new(
        RaftHandle::start_async(
            RaftCfg::new(1, vec![1], 1, 4, path.join("raft")),
            InMemoryNetwork::new().transport(1),
        )
        .await
        .unwrap(),
    );
    wait_leader(&raft).await;
    let registry = Arc::new(
        ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            path.join("raft/registry"),
            adapters(),
        )
        .await
        .unwrap(),
    );
    let mut logs = Vec::new();
    let mut bound: Vec<Arc<dyn StreamAdapter>> = Vec::new();
    let mut counters = Vec::new();
    for id in 1..=3 {
        let mut descriptor = descriptor(id);
        if id == 2 {
            descriptor.requires.push(StreamId(1));
        }
        registry
            .change(id as u128, RegistryOp::Register(descriptor.clone()))
            .await
            .unwrap();
        let dir = path.join(format!("stream-{id}"));
        let log = Arc::new(
            DurableStreamLog::open(dir.clone(), identity(), descriptor)
                .await
                .unwrap(),
        );
        let counter = Arc::new(Counter {
            path: dir.join("business"),
            owner: Mutex::new(()),
        });
        bound.push(Arc::new(LoggedStreamAdapter::new(
            log.clone(),
            counter.clone(),
        )));
        logs.push(log);
        counters.push(counter);
    }
    let group = GenericReplicationGroup::open(raft, registry.clone(), logs, Arc::new(NoPeers))
        .await
        .unwrap();
    let scheduler = StreamApplyScheduler::new(&registry.refresh().await.unwrap(), bound).unwrap();
    Fixture {
        group,
        registry,
        scheduler,
        counters,
    }
}

#[tokio::test]
async fn custom_streams_share_one_group_apply_dependencies_and_recover_results() {
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("custom-streams-{}", mudu_sys::random::uuid_v4()));
    let f = open(&path).await;
    let schema = f
        .group
        .append(StreamId(1), 101, vec![3], vec![])
        .await
        .unwrap();
    let data = f
        .group
        .append(StreamId(2), 102, vec![7], vec![schema.progress.clone()])
        .await
        .unwrap();
    f.group
        .append(StreamId(3), 103, vec![11], vec![])
        .await
        .unwrap();
    let state = f.registry.refresh().await.unwrap();
    assert_eq!(
        f.scheduler.apply_stream(&state, StreamId(2)).await.unwrap(),
        ApplyOutcome::Waiting
    );
    f.scheduler.apply_stream(&state, StreamId(3)).await.unwrap();
    assert_eq!(f.counters[2].load().await.value, 11);
    assert_eq!(f.counters[1].load().await.value, 0);
    f.scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    f.scheduler.apply_stream(&state, StreamId(2)).await.unwrap();
    assert_eq!(f.counters[1].load().await.results[&102], 7);
    f.registry.checkpoint().await.unwrap();
    f.group.shutdown().await.unwrap();
    drop(f);
    let f = open(&path).await;
    let retry = f
        .group
        .append(StreamId(2), 102, vec![7], vec![schema.progress])
        .await
        .unwrap();
    assert_eq!(retry.progress, data.progress);
    let recovered = f.registry.refresh().await.unwrap();
    for result in tokio::task::LocalSet::new()
        .run_until(f.scheduler.apply_round(&recovered))
        .await
        .unwrap()
        .values()
    {
        assert_eq!(result.as_ref().unwrap(), &ApplyOutcome::Current);
    }
    assert_eq!(f.counters[1].load().await.value, 7);
    assert_eq!(f.counters[1].load().await.results.len(), 1);
    f.group.shutdown().await.unwrap();
    drop(f);
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&path)
        .await
        .unwrap();
}

#[tokio::test]
async fn invalid_dependency_is_rejected_before_it_can_poison_the_committed_stream() {
    let path = mudu_sys::env_var::temp_dir().join(format!(
        "dependency-admission-{}",
        mudu_sys::random::uuid_v4()
    ));
    let f = open(&path).await;
    let schema = f
        .group
        .append(StreamId(1), 100, vec![1], vec![])
        .await
        .unwrap();
    for case in 0..3 {
        let mut invalid = schema.progress.clone();
        match case {
            0 => invalid.stream_generation += 1,
            1 => invalid.segment_digest[0] ^= 1,
            _ => invalid.end_lsn += 1,
        }
        assert!(f
            .group
            .append(StreamId(2), 101, vec![2], vec![invalid])
            .await
            .is_err());
        assert!(!f
            .registry
            .refresh()
            .await
            .unwrap()
            .committed()
            .contains_key(&StreamId(2)));
    }
    let valid = f
        .group
        .append(StreamId(2), 101, vec![2], vec![schema.progress])
        .await
        .unwrap();
    assert_eq!(valid.progress.end_lsn, 0);
    f.group.shutdown().await.unwrap();
    drop(f);
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&path)
        .await
        .unwrap();
}

/// One single-node group with one custom counter stream.
struct SplitFixture {
    group: GenericReplicationGroup,
    counter: Arc<Counter>,
    path: PathBuf,
}

async fn open_split(path: PathBuf, group_id: u64, request_base: u128) -> SplitFixture {
    let identity = GroupIdobject {
        cluster_id: 7,
        group_id: GroupId(group_id),
        generation: 1,
    };
    let raft = Arc::new(
        RaftHandle::start_async(
            RaftCfg::new(group_id, vec![group_id], 1, 4, path.join("raft")),
            InMemoryNetwork::new().transport(group_id),
        )
        .await
        .unwrap(),
    );
    wait_leader(&raft).await;
    let registry = Arc::new(
        ReplicatedStreamRegistry::open(identity, raft.clone(), path.join("registry"), adapters())
            .await
            .unwrap(),
    );
    registry
        .change(request_base, RegistryOp::Register(descriptor(1)))
        .await
        .unwrap();
    let log = Arc::new(
        DurableStreamLog::open(path.join("stream"), identity, descriptor(1))
            .await
            .unwrap(),
    );
    let counter = Arc::new(Counter {
        path: path.join("business"),
        owner: Mutex::new(()),
    });
    let group =
        GenericReplicationGroup::open(raft, registry.clone(), vec![log.clone()], Arc::new(NoPeers))
            .await
            .unwrap();
    let _adapter = Arc::new(LoggedStreamAdapter::new(log, counter.clone()));
    SplitFixture {
        group,
        counter,
        path,
    }
}

#[tokio::test]
async fn custom_streams_run_in_split_groups_independently() {
    let base =
        mudu_sys::env_var::temp_dir().join(format!("split-groups-{}", mudu_sys::random::uuid_v4()));
    let left = open_split(base.join("left"), 21, 900).await;
    let right = open_split(base.join("right"), 22, 800).await;
    // Each group commits its own stream; group identity separates them.
    let a = left
        .group
        .append(StreamId(1), 901, vec![5], vec![])
        .await
        .unwrap();
    let b = right
        .group
        .append(StreamId(1), 801, vec![9], vec![])
        .await
        .unwrap();
    // Stream ids are group-scoped: both groups use StreamId(1) without
    // confusion; their registries carry distinct group identities.
    assert_eq!(a.progress.end_lsn, 0);
    assert_eq!(b.progress.end_lsn, 0);
    assert_eq!(left.counter.load().await.value, 0);
    // Stop the left group: the right group keeps serving.
    left.group.shutdown().await.unwrap();
    right
        .group
        .append(StreamId(1), 802, vec![13], vec![])
        .await
        .unwrap();
    assert_eq!(right.counter.load().await.value, 0);
    right.group.shutdown().await.unwrap();
    // Metrics are keyed by (group, stream): both appends recorded separately.
    let snapshot = StreamMetrics::global().snapshot();
    assert!(snapshot.contains_key(&(GroupId(21), StreamId(1))));
    assert!(snapshot.contains_key(&(GroupId(22), StreamId(1))));
    for dir in [left.path, right.path] {
        mudu_sys::fs::tokio_fs().remove_dir_all(&dir).await.unwrap();
    }
}

#[tokio::test]
async fn logged_adapter_restores_log_and_business_into_fresh_target() {
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("logged-restore-{}", mudu_sys::random::uuid_v4()));
    let raft = Arc::new(
        RaftHandle::start_async(
            RaftCfg::new(1, vec![1], 1, 4, path.join("raft")),
            InMemoryNetwork::new().transport(1),
        )
        .await
        .unwrap(),
    );
    wait_leader(&raft).await;
    let registry = Arc::new(
        ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            path.join("raft/registry"),
            adapters(),
        )
        .await
        .unwrap(),
    );
    registry
        .change(1, RegistryOp::Register(descriptor(1)))
        .await
        .unwrap();
    let log = Arc::new(
        DurableStreamLog::open(path.join("stream"), identity(), descriptor(1))
            .await
            .unwrap(),
    );
    let counter = Arc::new(Counter {
        path: path.join("business"),
        owner: Mutex::new(()),
    });
    let adapter = Arc::new(LoggedStreamAdapter::new(log.clone(), counter.clone()));
    let group = GenericReplicationGroup::open(
        raft.clone(),
        registry.clone(),
        vec![log.clone()],
        Arc::new(NoPeers),
    )
    .await
    .unwrap();
    let scheduler =
        StreamApplyScheduler::new(&registry.refresh().await.unwrap(), vec![adapter.clone()])
            .unwrap();
    let receipt = group.append(StreamId(1), 7, vec![5], vec![]).await.unwrap();
    let state = registry.refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    assert_eq!(counter.load().await.value, 5);

    // Capture the complete logged snapshot at the applied cut.
    let bytes = adapter.snapshot_at(Some(&receipt.progress)).await.unwrap();
    let image =
        LoggedStreamSnapshot::decode(&bytes, identity(), &descriptor(1), Some(&receipt.progress))
            .unwrap();

    // Restore into a fresh, empty log and business target.
    let fresh_log = Arc::new(
        DurableStreamLog::open(path.join("fresh"), identity(), descriptor(1))
            .await
            .unwrap(),
    );
    let fresh_counter = Arc::new(Counter {
        path: path.join("fresh-business"),
        owner: Mutex::new(()),
    });
    let fresh = Arc::new(LoggedStreamAdapter::new(
        fresh_log.clone(),
        fresh_counter.clone(),
    ));
    fresh.restore_snapshot(&image).await.unwrap();
    assert_eq!(fresh_counter.load().await.value, 5);
    assert!(fresh_log.contains(&receipt.progress).await.unwrap());
    // Identical retry is idempotent; the durable retry history is preserved.
    fresh.restore_snapshot(&image).await.unwrap();

    group.shutdown().await.unwrap();
    drop(scheduler);
    drop(fresh);
    drop(adapter);
    drop(group);
    drop(registry);
    drop(raft);
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&path)
        .await
        .unwrap();
}
