//! Stream-registry and vector recovery regression tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use mudu::common::result::RS;
use mudu_raft::stream::*;
use mudu_raft::{
    ConfVersion, InMemoryNetwork, RaftCfg, RaftHandle, RaftRole, ReplEntry, TermIndex,
};
use std::sync::Arc;
use std::time::Duration;

pub(super) fn identity() -> GroupIdobject {
    GroupIdobject {
        cluster_id: 7,
        group_id: GroupId(11),
        generation: 1,
    }
}
pub(super) fn adapters() -> AdapterCapabilities {
    let mut capabilities = AdapterCapabilities::default();
    capabilities.add("test.counter", 1, 1);
    capabilities
}
pub(super) fn descriptor(id: u64) -> StreamDescriptor {
    StreamDescriptor {
        stream_id: StreamId(id),
        stream_generation: 1,
        kind: "test.counter".into(),
        codec_version: 1,
        digest_version: 1,
        budget: StreamBudget {
            max_record_bytes: 1024,
            max_inflight_bytes: 8192,
            priority: 1,
        },
        requires: Vec::new(),
    }
}
pub(super) fn change(state: &GroupState, request_id: u128, operation: RegistryOp) -> ReplEntry {
    ReplEntry::StreamRegistry(RegistryChange {
        format_version: REGISTRY_FORMAT_VERSION,
        identity: identity(),
        request_id,
        previous_version: state.registry_version(),
        operation,
    })
}
pub(super) fn apply(state: &mut GroupState, entry: &ReplEntry) -> RS<()> {
    state.apply(
        TermIndex {
            term: 1,
            index: state.frontier().index + 1,
        },
        entry,
        &adapters(),
    )
}
fn register(state: &mut GroupState, id: u64) {
    let entry = change(state, id as u128 + 1, RegistryOp::Register(descriptor(id)));
    apply(state, &entry).unwrap();
}
pub(super) fn progress(id: u64, lsn: u64) -> StreamProgress {
    StreamProgress {
        stream_id: StreamId(id),
        stream_generation: 1,
        end_lsn: lsn,
        segment_digest: [lsn as u8; 32],
    }
}
pub(super) fn commit(state: &GroupState, progress: Vec<StreamProgress>) -> ReplEntry {
    let version = ConfVersion {
        term: 1,
        version: 1,
        index: 0,
    };
    ReplEntry::GroupCommit(GroupCommit {
        cluster_id: identity().cluster_id,
        group_id: identity().group_id,
        group_generation: 1,
        membership_version: MembershipVersion {
            committed: version.clone(),
            incoming: version,
        },
        registry_version: state.registry_version(),
        progress,
    })
}

#[test]
fn delta_vectors_preserve_other_streams_and_validate_atomically() {
    let mut state = GroupState::new(identity()).unwrap();
    register(&mut state, 0);
    register(&mut state, 1);
    let entry = commit(&state, vec![progress(0, 0), progress(1, 3)]);
    apply(&mut state, &entry).unwrap();
    let entry = commit(&state, vec![progress(0, 7)]);
    apply(&mut state, &entry).unwrap();
    assert_eq!(state.committed()[&StreamId(1)].end_lsn, 3);
    let before = rmp_serde::to_vec(&state).unwrap();
    let mut conflict = progress(1, 3);
    conflict.segment_digest = [99; 32];
    let invalid_vectors = [
        vec![progress(0, 8), conflict],
        vec![progress(0, 6)],
        vec![progress(0, 8), progress(0, 9)],
        vec![progress(99, 1)],
        Vec::new(),
    ];
    for delta in invalid_vectors {
        let entry = commit(&state, delta);
        assert!(apply(&mut state, &entry).is_err());
        assert_eq!(rmp_serde::to_vec(&state).unwrap(), before);
    }
    state.validate(&adapters()).unwrap();
}

#[test]
fn registry_retries_preserve_results_and_reject_request_identity_reuse() {
    let mut state = GroupState::new(identity()).unwrap();
    let entry = change(&state, 17, RegistryOp::Register(descriptor(1)));
    apply(&mut state, &entry).unwrap();
    apply(&mut state, &entry).unwrap();
    assert_eq!(state.registry_version(), 1);
    assert_eq!(
        state
            .receipt(17, &RegistryOp::Register(descriptor(1)))
            .unwrap(),
        Some(1)
    );
    let collision = change(&state, 17, RegistryOp::Register(descriptor(2)));
    assert!(apply(&mut state, &collision).is_err());
    let encoded = rmp_serde::to_vec(&state).unwrap();
    let recovered: GroupState = rmp_serde::from_slice(&encoded).unwrap();
    recovered.validate(&adapters()).unwrap();
    assert_eq!(
        recovered
            .receipt(17, &RegistryOp::Register(descriptor(1)))
            .unwrap(),
        Some(1)
    );
}

#[test]
fn lifecycle_keeps_tombstones_and_protects_dependency_references() {
    let mut state = GroupState::new(identity()).unwrap();
    register(&mut state, 1);
    let mut child = descriptor(2);
    child.requires.push(StreamId(1));
    let entry = change(&state, 3, RegistryOp::Register(child));
    apply(&mut state, &entry).unwrap();
    let entry = change(
        &state,
        4,
        RegistryOp::Deactivate {
            stream_id: StreamId(1),
            stream_generation: 1,
        },
    );
    apply(&mut state, &entry).unwrap();
    let remove_parent = RegistryOp::Remove {
        stream_id: StreamId(1),
        stream_generation: 1,
    };
    let entry = change(&state, 5, remove_parent.clone());
    assert!(apply(&mut state, &entry).is_err());
    let entry = change(
        &state,
        6,
        RegistryOp::Deactivate {
            stream_id: StreamId(2),
            stream_generation: 1,
        },
    );
    apply(&mut state, &entry).unwrap();
    let entry = change(
        &state,
        7,
        RegistryOp::Remove {
            stream_id: StreamId(2),
            stream_generation: 1,
        },
    );
    apply(&mut state, &entry).unwrap();
    let entry = change(&state, 8, remove_parent);
    apply(&mut state, &entry).unwrap();
    let mut reused = descriptor(2);
    reused.stream_generation = 2;
    let entry = change(&state, 9, RegistryOp::Register(reused));
    assert!(apply(&mut state, &entry).is_err());
    assert_eq!(
        state.streams()[&StreamId(2)].lifecycle,
        StreamLifecycle::Tombstoned
    );
    state.validate(&adapters()).unwrap();
}

#[test]
fn nonempty_stream_cannot_be_removed_without_retirement_evidence() {
    let mut state = GroupState::new(identity()).unwrap();
    register(&mut state, 1);
    let entry = commit(&state, vec![progress(1, 0)]);
    apply(&mut state, &entry).unwrap();
    let entry = change(
        &state,
        3,
        RegistryOp::Deactivate {
            stream_id: StreamId(1),
            stream_generation: 1,
        },
    );
    apply(&mut state, &entry).unwrap();
    let entry = change(
        &state,
        4,
        RegistryOp::Remove {
            stream_id: StreamId(1),
            stream_generation: 1,
        },
    );
    assert!(apply(&mut state, &entry).is_err());
}

#[test]
fn recovery_rejects_missing_codecs_cycles_and_foreign_generations() {
    let mut state = GroupState::new(identity()).unwrap();
    register(&mut state, 1);
    register(&mut state, 2);
    assert!(state.validate(&AdapterCapabilities::default()).is_err());
    let mut cyclic = state.clone();
    cyclic
        .streams
        .get_mut(&StreamId(1))
        .unwrap()
        .descriptor
        .requires
        .push(StreamId(2));
    cyclic
        .streams
        .get_mut(&StreamId(2))
        .unwrap()
        .descriptor
        .requires
        .push(StreamId(1));
    assert!(cyclic.validate(&adapters()).is_err());
    let mut foreign = commit(&state, vec![progress(1, 3)]);
    if let ReplEntry::GroupCommit(commit) = &mut foreign {
        commit.group_generation += 1;
    }
    assert!(apply(&mut state, &foreign).is_err());
    let mut stale = progress(1, 3);
    stale.stream_generation += 1;
    let entry = commit(&state, vec![stale]);
    assert!(apply(&mut state, &entry).is_err());
    assert!(state
        .apply(
            TermIndex { term: 1, index: 99 },
            &ReplEntry::Barrier { term: 1 },
            &adapters()
        )
        .is_err());
}

pub(super) fn start_node(path: &std::path::Path) -> Arc<RaftHandle> {
    Arc::new(
        RaftHandle::start(
            RaftCfg::new(1, vec![1], 1, 4, path.to_path_buf()),
            InMemoryNetwork::new().transport(1),
        )
        .unwrap(),
    )
}
pub(super) async fn wait_leader(raft: &RaftHandle) {
    let mut status = raft.watch_status_async().unwrap();
    mudu_sys::timeout(Duration::from_secs(10), async {
        loop {
            if status.borrow_and_update().role == RaftRole::Leader {
                break;
            }
            status.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[test]
fn raft_registry_checkpoint_and_delta_suffix_survive_restart() {
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("stream-registry-{}", mudu_sys::random::uuid_v4()));
    let checkpoint = path.join("registry.checkpoint");
    let raft = start_node(&path);
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        wait_leader(&raft).await;
        let service = ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            checkpoint.clone(),
            adapters(),
        )
        .await
        .unwrap();
        assert_eq!(
            service
                .change(1, RegistryOp::Register(descriptor(1)))
                .await
                .unwrap(),
            1
        );
        service
            .change(2, RegistryOp::Register(descriptor(2)))
            .await
            .unwrap();
        service.checkpoint().await.unwrap();
        let state = service.refresh().await.unwrap();
        raft.propose(commit(&state, vec![progress(1, 4), progress(2, 7)]))
            .unwrap()
            .wait()
            .await
            .unwrap();
        let state = service.refresh().await.unwrap();
        raft.propose(commit(&state, vec![progress(1, 9)]))
            .unwrap()
            .wait()
            .await
            .unwrap();
        let before = raft.commit_index();
        assert_eq!(
            service
                .change(1, RegistryOp::Register(descriptor(1)))
                .await
                .unwrap(),
            1
        );
        assert_eq!(raft.commit_index(), before);
        raft.stop_async().await.unwrap();
    });
    drop(raft);
    let raft = start_node(&path);
    runtime.block_on(async {
        let service = ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            checkpoint.clone(),
            adapters(),
        )
        .await
        .unwrap();
        let state = service.refresh().await.unwrap();
        assert_eq!(state.committed()[&StreamId(1)].end_lsn, 9);
        assert_eq!(state.committed()[&StreamId(2)].end_lsn, 7);
        assert_eq!(
            service
                .change(1, RegistryOp::Register(descriptor(1)))
                .await
                .unwrap(),
            1
        );
        assert!(ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            checkpoint.clone(),
            AdapterCapabilities::default()
        )
        .await
        .is_err());
        raft.stop_async().await.unwrap();
    });
    drop(raft);
    mudu_sys::fs::sync::remove_dir_all(path).unwrap();
}

#[test]
fn compacted_registry_needs_a_valid_checkpoint_and_rejects_corruption() {
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("registry-compact-{}", mudu_sys::random::uuid_v4()));
    let checkpoint = path.join("registry.ckpt");
    let raft = start_node(&path);
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        wait_leader(&raft).await;
        let service = ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            checkpoint.clone(),
            adapters(),
        )
        .await
        .unwrap();
        service
            .change(1, RegistryOp::Register(descriptor(1)))
            .await
            .unwrap();
        service.checkpoint().await.unwrap();
        // This group contains registry metadata only: there is no business
        // payload frontier whose snapshot would be needed before compaction.
        raft.force_compact_async().await.unwrap();
        let restored = ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            checkpoint.clone(),
            adapters(),
        )
        .await
        .unwrap();
        assert_eq!(restored.refresh().await.unwrap().registry_version(), 1);
        assert!(ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            path.join("missing.ckpt"),
            adapters()
        )
        .await
        .is_err());
        let fs = mudu_sys::fs::tokio_fs();
        let mut bytes = fs.read_all(&checkpoint).await.unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        let file = fs
            .open(
                &checkpoint,
                mudu_sys::contract::file_options::FileOptions::read_write_create(),
            )
            .await
            .unwrap();
        file.write_all_at(0, &bytes).await.unwrap();
        file.fsync().await.unwrap();
        assert!(ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            checkpoint.clone(),
            adapters()
        )
        .await
        .is_err());
        service.checkpoint().await.unwrap();
        assert!(ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            checkpoint.clone(),
            adapters()
        )
        .await
        .is_ok());
        raft.stop_async().await.unwrap();
    });
    drop(raft);
    mudu_sys::fs::sync::remove_dir_all(path).unwrap();
}

async fn wait_index(raft: &RaftHandle, index: u64) {
    let mut status = raft.watch_status_async().unwrap();
    mudu_sys::timeout(Duration::from_secs(10), async {
        loop {
            if status.borrow_and_update().commit_index >= index {
                break;
            }
            status.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[test]
fn registry_mutation_results_survive_three_node_leader_failover() {
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("registry-failover-{}", mudu_sys::random::uuid_v4()));
    let network = InMemoryNetwork::new();
    let mut nodes = Vec::new();
    for id in 1..=3 {
        let mut cfg = RaftCfg::new(id, vec![1, 2, 3], 1, 4, path.join(id.to_string()));
        cfg.manual_ticks = true;
        let raft = Arc::new(RaftHandle::start(cfg, network.transport(id)).unwrap());
        network.register(id, raft.input_sender()).unwrap();
        nodes.push(raft);
    }
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        for _ in 0..6 {
            nodes[0].inject_tick().unwrap();
        }
        wait_leader(&nodes[0]).await;
        let first = ReplicatedStreamRegistry::open(
            identity(),
            nodes[0].clone(),
            path.join("1/registry.ckpt"),
            adapters(),
        )
        .await
        .unwrap();
        first
            .change(101, RegistryOp::Register(descriptor(1)))
            .await
            .unwrap();
        let frontier = nodes[0].commit_index();
        for _ in 0..4 {
            nodes[0].inject_tick().unwrap();
        }
        for node in &nodes[1..] {
            wait_index(node, frontier).await;
        }
        network.unregister(1).unwrap();
        nodes[0].stop_async().await.unwrap();
        for _ in 0..6 {
            nodes[1].inject_tick().unwrap();
        }
        wait_leader(&nodes[1]).await;
        let successor = ReplicatedStreamRegistry::open(
            identity(),
            nodes[1].clone(),
            path.join("2/registry.ckpt"),
            adapters(),
        )
        .await
        .unwrap();
        assert_eq!(
            successor
                .change(101, RegistryOp::Register(descriptor(1)))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            successor
                .change(102, RegistryOp::Register(descriptor(2)))
                .await
                .unwrap(),
            2
        );
        let frontier = nodes[1].commit_index();
        for _ in 0..4 {
            nodes[1].inject_tick().unwrap();
        }
        wait_index(&nodes[2], frontier).await;
        let follower = ReplicatedStreamRegistry::open(
            identity(),
            nodes[2].clone(),
            path.join("3/registry.ckpt"),
            adapters(),
        )
        .await
        .unwrap();
        assert_eq!(follower.refresh().await.unwrap().streams().len(), 2);
        for node in &nodes[1..] {
            node.stop_async().await.unwrap();
        }
    });
    drop(nodes);
    mudu_sys::fs::sync::remove_dir_all(path).unwrap();
}

#[test]
fn checkpoint_validation_rejects_registry_history_and_cursor_tampering() {
    let mut state = GroupState::new(identity()).unwrap();
    register(&mut state, 1);
    let mut changed = state.clone();
    changed
        .streams
        .get_mut(&StreamId(1))
        .unwrap()
        .descriptor
        .budget
        .priority += 1;
    assert!(changed.validate(&adapters()).is_err());
    let mut changed = state.clone();
    changed.cursor = TermIndex { index: 0, term: 0 };
    assert!(changed.validate(&adapters()).is_err());
    let mut changed = state.clone();
    changed.receipts.values_mut().next().unwrap().version += 1;
    assert!(changed.validate(&adapters()).is_err());
    let before = state.frontier();
    assert!(state
        .apply(
            TermIndex {
                term: 0,
                index: before.index + 1
            },
            &ReplEntry::Barrier { term: 0 },
            &adapters()
        )
        .is_err());
    assert_eq!(state.frontier(), before);
}

#[test]
fn concurrent_registry_requests_serialize_and_request_collision_does_not_poison_log() {
    let path = mudu_sys::env_var::temp_dir().join(format!(
        "registry-concurrent-{}",
        mudu_sys::random::uuid_v4()
    ));
    let raft = start_node(&path);
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        wait_leader(&raft).await;
        let service = Arc::new(
            ReplicatedStreamRegistry::open(
                identity(),
                raft.clone(),
                path.join("registry.ckpt"),
                adapters(),
            )
            .await
            .unwrap(),
        );
        let left = {
            let service = service.clone();
            mudu_sys::tokio::spawn(async move {
                service
                    .change(100, RegistryOp::Register(descriptor(1)))
                    .await
            })
        };
        let right = {
            let service = service.clone();
            mudu_sys::tokio::spawn(async move {
                service
                    .change(100, RegistryOp::Register(descriptor(2)))
                    .await
            })
        };
        let results = [left.await.unwrap(), right.await.unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(service.refresh().await.unwrap().registry_version(), 1);
        let left = {
            let service = service.clone();
            mudu_sys::tokio::spawn(async move {
                service
                    .change(101, RegistryOp::Register(descriptor(3)))
                    .await
            })
        };
        let right = {
            let service = service.clone();
            mudu_sys::tokio::spawn(async move {
                service
                    .change(102, RegistryOp::Register(descriptor(4)))
                    .await
            })
        };
        let mut versions = [left.await.unwrap().unwrap(), right.await.unwrap().unwrap()];
        versions.sort_unstable();
        assert_eq!(versions, [2, 3]);
        let state = service.refresh().await.unwrap();
        assert_eq!(state.streams().len(), 3);
        state.validate(&adapters()).unwrap();
        service.checkpoint().await.unwrap();
        raft.stop_async().await.unwrap();
    });
    drop(raft);
    mudu_sys::fs::sync::remove_dir_all(path).unwrap();
}

#[test]
fn registry_replay_tolerates_business_entries_on_a_shared_log() {
    let mut state = GroupState::new(identity()).unwrap();
    register(&mut state, 1);
    // CommitVec / XidAlloc entries on a shared data-group log advance the
    // cursor without touching the registry state.
    apply(
        &mut state,
        &ReplEntry::CommitVec {
            term: 1,
            per_worker: vec![(0, 7, [1u8; 32])],
        },
    )
    .unwrap();
    apply(
        &mut state,
        &ReplEntry::XidAlloc {
            worker_id: 0,
            base: 100,
            count: 16,
        },
    )
    .unwrap();
    let entry = commit(&state, vec![progress(1, 4)]);
    apply(&mut state, &entry).unwrap();
    assert_eq!(state.committed()[&StreamId(1)].end_lsn, 4);
    assert_eq!(state.registry_version(), 1);
}

#[test]
fn group_snapshot_installs_raft_frontier_and_registry_into_a_fresh_node() {
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("group-install-{}", mudu_sys::random::uuid_v4()));
    let source = start_node(&path.join("source"));
    let fresh = start_node(&path.join("fresh"));
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        wait_leader(&source).await;
        let registry = ReplicatedStreamRegistry::open(
            identity(),
            source.clone(),
            path.join("source/registry"),
            adapters(),
        )
        .await
        .unwrap();
        registry
            .change(1, RegistryOp::Register(descriptor(1)))
            .await
            .unwrap();
        let state = registry.refresh().await.unwrap();
        source
            .propose(commit(&state, vec![progress(1, 3)]))
            .unwrap()
            .wait()
            .await
            .unwrap();
        let state = registry.refresh().await.unwrap();
        registry.checkpoint().await.unwrap();

        // A fresh node installs the Raft snapshot frontier, then the registry
        // checkpoint at the same cut, then reopens against both.
        wait_leader(&fresh).await;
        fresh
            .install_snapshot_async(state.frontier())
            .await
            .unwrap();
        install_registry_checkpoint(
            identity(),
            &fresh,
            &path.join("fresh/registry"),
            &adapters(),
            &state,
        )
        .await
        .unwrap();
        let reopened = ReplicatedStreamRegistry::open(
            identity(),
            fresh.clone(),
            path.join("fresh/registry"),
            adapters(),
        )
        .await
        .unwrap();
        assert_eq!(
            reopened.refresh().await.unwrap().committed(),
            state.committed()
        );
        assert_eq!(
            reopened.refresh().await.unwrap().streams().len(),
            state.streams().len()
        );

        // A foreign-generation state is rejected before touching the checkpoint.
        let mut foreign = state.clone();
        foreign.identity.generation += 1;
        assert!(install_registry_checkpoint(
            identity(),
            &fresh,
            &path.join("fresh/foreign"),
            &adapters(),
            &foreign
        )
        .await
        .is_err());

        source.stop_async().await.unwrap();
        fresh.stop_async().await.unwrap();
    });
    drop(source);
    drop(fresh);
    mudu_sys::fs::sync::remove_dir_all(path).unwrap();
}
