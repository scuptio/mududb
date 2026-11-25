use super::*;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct View {
    prefix: Option<StreamProgress>,
    requests: BTreeMap<u128, Vec<u8>>,
}
#[derive(Default)]
struct Machine(Mutex<View>);
#[async_trait]
impl StreamStateMachine for Machine {
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(self.0.lock().await.prefix.clone())
    }
    async fn apply_once(&self, id: u128, record: StreamStruct) -> RS<()> {
        let mut view = self.0.lock().await;
        view.requests.insert(id, record.payload);
        view.prefix = Some(record.progress);
        Ok(())
    }
    async fn snapshot_at(&self, prefix: Option<&StreamProgress>) -> RS<Vec<u8>> {
        let view = self.0.lock().await;
        assert_eq!(view.prefix.as_ref(), prefix);
        crate::log_stream::frontier::encode(&*view)
    }
}

#[tokio::test]
async fn custom_stream_snapshot_uses_real_group_barrier_and_retains_results() {
    let c = cluster().await;
    let machine = Arc::new(Machine::default());
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            machine.clone(),
        ))],
    )
    .unwrap();
    // Empty registered streams are captured, not omitted from the bundle.
    let empty = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .unwrap();
    assert!(empty.manifest.images[&StreamId(1)].prefix.is_none());
    c.groups[0]
        .append(StreamId(1), 77, b"retained-result".to_vec(), vec![])
        .await
        .unwrap();
    // Committed is not sufficient: business apply must reach the same prefix.
    assert!(c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .is_err());
    let state = c.registries[0].refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    let bundle = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .unwrap();
    assert!(c.groups[1]
        .capture_snapshot_bundle(&scheduler)
        .await
        .is_err());
    let path = c.path.join("bundle");
    bundle.publish(&path, &adapters()).await.unwrap();
    let loaded = GroupSnapshotBundle::read(&path, identity(), &adapters())
        .await
        .unwrap();
    let decoded = loaded.decode_logged_images(&adapters()).unwrap();
    let image = &decoded[&StreamId(1)];
    let restored_log =
        DurableStreamLog::open(c.path.join("restored-log"), identity(), descriptor(1))
            .await
            .unwrap();
    restored_log.restore_snapshot(image.log()).await.unwrap();
    assert_eq!(
        restored_log.request_progress(77).await.unwrap().as_ref(),
        image.log().prefix()
    );
    assert!(restored_log
        .append(77, b"conflict".to_vec(), vec![])
        .await
        .is_err());
    let restored: View = crate::log_stream::frontier::decode(image.business()).unwrap();
    assert_eq!(restored.requests[&77], b"retained-result");
    assert_eq!(restored.prefix, loaded.manifest.images[&StreamId(1)].prefix);
    let restored = Arc::new(Machine(Mutex::new(restored)));
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            restored.clone(),
        ))],
    )
    .unwrap();
    c.groups[0]
        .append(StreamId(1), 78, b"after-restore".to_vec(), vec![])
        .await
        .unwrap();
    let state = c.registries[0].refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    assert_eq!(restored.0.lock().await.requests.len(), 2);
    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}

struct Unsupported;
#[async_trait]
impl StreamStateMachine for Unsupported {
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(None)
    }
    async fn apply_once(&self, _: u128, _: StreamStruct) -> RS<()> {
        Ok(())
    }
}

#[tokio::test]
async fn unsupported_business_snapshot_does_not_fabricate_an_empty_image() {
    let c = cluster().await;
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            Arc::new(Unsupported),
        ))],
    )
    .unwrap();
    let result = c.groups[0].capture_snapshot_bundle(&scheduler).await;
    assert_eq!(
        result.err().unwrap().ec(),
        mudu::error::ErrorCode::NotImplemented
    );
    // Failed capture releases stream owners; subsequent appends still complete.
    c.groups[0]
        .append(StreamId(1), 100, vec![1], vec![])
        .await
        .unwrap();
    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}

struct ChangeRegistry(Arc<ReplicatedStreamRegistry>);
#[async_trait]
impl StreamStateMachine for ChangeRegistry {
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(None)
    }
    async fn apply_once(&self, _: u128, _: StreamStruct) -> RS<()> {
        Ok(())
    }
    async fn snapshot_at(&self, _: Option<&StreamProgress>) -> RS<Vec<u8>> {
        self.0
            .change(999, RegistryOp::Register(descriptor(2)))
            .await?;
        Ok(vec![])
    }
}

#[tokio::test]
async fn registry_change_during_capture_invalidates_the_bundle() {
    let c = cluster().await;
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            Arc::new(ChangeRegistry(c.registries[0].clone())),
        ))],
    )
    .unwrap();
    let error = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .err()
        .unwrap();
    assert!(error
        .message()
        .contains("group changed during snapshot capture"));
    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}

#[tokio::test]
async fn group_snapshot_bundle_ships_over_the_stream_transport() {
    let c = cluster().await;
    let machine = Arc::new(Machine::default());
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            machine.clone(),
        ))],
    )
    .unwrap();
    c.groups[0]
        .append(StreamId(1), 77, b"ship".to_vec(), vec![])
        .await
        .unwrap();
    let state = c.registries[0].refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    let bundle = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .unwrap();
    bundle
        .publish(&c.path.join("bundle"), &adapters())
        .await
        .unwrap();

    // The leader serves its captured bundle to a RaftFollower that fell below the
    // retained prefix, over the group's stream transport.
    let served = c.groups[0].serve_snapshot(identity()).await.unwrap();
    let decoded = GroupSnapshotBundle::decode(&served, identity(), &adapters()).unwrap();
    assert_eq!(
        decoded.manifest.state.committed(),
        bundle.manifest.state.committed()
    );

    // A replica fetches the same bundle from the leader over the transport.
    let fetched = c.groups[1].fetch_snapshot_from(1).await.unwrap();
    let decoded = GroupSnapshotBundle::decode(&fetched, identity(), &adapters()).unwrap();
    assert_eq!(
        decoded.manifest.state.committed(),
        bundle.manifest.state.committed()
    );
    assert!(decoded
        .decode_logged_images(&adapters())
        .unwrap()
        .contains_key(&StreamId(1)));

    // A foreign group request is rejected.
    let mut foreign = identity();
    foreign.generation += 1;
    assert!(c.groups[0].serve_snapshot(foreign).await.is_err());

    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}

#[tokio::test]
async fn lagging_replica_installs_a_snapshot_fetched_over_the_transport() {
    let c = cluster().await;
    let machine = Arc::new(Machine::default());
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            machine.clone(),
        ))],
    )
    .unwrap();
    // Node 3 is isolated from the payload push, so its external log stays empty
    // while its registry still learns the committed prefix over Raft.
    c.network.blocked.lock().await.insert(3);
    c.groups[0]
        .append(StreamId(1), 77, b"lag".to_vec(), vec![])
        .await
        .unwrap();
    let state = c.registries[0].refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    let bundle = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .unwrap();
    let bundle_dir = c.path.join("bundle");
    bundle.publish(&bundle_dir, &adapters()).await.unwrap();
    assert!(c.logs[2].frontier().await.unwrap().is_none());

    // Node 3 recovers by fetching + installing the snapshot from the leader.
    c.network.blocked.lock().await.remove(&3);
    c.groups[2]
        .install_snapshot_from_peer(1, &bundle_dir, &c.path.join("journal-3"))
        .await
        .unwrap();
    let committed = bundle.manifest.state.committed()[&StreamId(1)].clone();
    assert!(c.logs[2].contains(&committed).await.unwrap());

    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}

#[tokio::test]
async fn leader_gcs_below_the_bundle_and_a_lagging_replica_still_recovers() {
    let c = cluster().await;
    let machine = Arc::new(Machine::default());
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            machine.clone(),
        ))],
    )
    .unwrap();
    c.network.blocked.lock().await.insert(3);
    c.groups[0]
        .append(StreamId(1), 77, b"gc".to_vec(), vec![])
        .await
        .unwrap();
    let state = c.registries[0].refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    let bundle = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .unwrap();
    let bundle_dir = c.path.join("bundle");
    bundle.publish(&bundle_dir, &adapters()).await.unwrap();
    c.groups[0]
        .install_snapshot_frontier(&bundle)
        .await
        .unwrap();

    // The leader reclaims the record below the bundle cut; it is now only
    // recoverable from the published bundle.
    let reclaimed = c.groups[0].gc_below_bundle(&bundle).await.unwrap();
    assert_eq!(reclaimed.len(), 1);
    assert!(c.logs[0].record(0).await.is_err());

    // The lagging replica recovers the reclaimed record from the bundle.
    c.network.blocked.lock().await.remove(&3);
    c.groups[2]
        .install_snapshot_from_peer(1, &bundle_dir, &c.path.join("journal-3"))
        .await
        .unwrap();
    let committed = bundle.manifest.state.committed()[&StreamId(1)].clone();
    assert!(c.logs[2].contains(&committed).await.unwrap());

    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}

#[tokio::test]
async fn group_install_journal_restores_a_fresh_node_and_redoes_on_startup() {
    let c = cluster().await;
    let machine = Arc::new(Machine::default());
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            machine.clone(),
        ))],
    )
    .unwrap();
    c.groups[0]
        .append(StreamId(1), 77, b"x".to_vec(), vec![])
        .await
        .unwrap();
    let state = c.registries[0].refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();
    let bundle = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .unwrap();
    let bundle_dir = c.path.join("bundle");
    bundle.publish(&bundle_dir, &adapters()).await.unwrap();

    // A fresh target node plus its empty external log.
    let fresh_raft = Arc::new(
        RaftHandle::start_async(
            RaftCfg::new(1, vec![1], 1, 4, c.path.join("fresh-raft")),
            InMemoryNetwork::new().transport(1),
        )
        .await
        .unwrap(),
    );
    wait_leader(&fresh_raft).await;
    let fresh_log = Arc::new(
        DurableStreamLog::open(c.path.join("fresh-log"), identity(), descriptor(1))
            .await
            .unwrap(),
    );
    let mut fresh_logs = BTreeMap::new();
    fresh_logs.insert(StreamId(1), fresh_log.clone());
    let target_dir = c.path.join("fresh-target");
    let checkpoint_path = c.path.join("fresh-raft/registry");

    // An install interrupted before completion leaves a durable pending intent.
    let journal = GroupInstallJournal::begin(&target_dir, &bundle_dir, &bundle)
        .await
        .unwrap();
    journal
        .install(&fresh_raft, &checkpoint_path, &adapters(), &fresh_logs)
        .await
        .unwrap();
    drop(journal);
    assert!(ensure_no_pending_group_install(&target_dir).await.is_err());

    // Startup redo revalidates the intent against the retained bundle and
    // completes the install.
    assert!(recover_pending_group_install(
        &target_dir,
        &fresh_raft,
        &checkpoint_path,
        &adapters(),
        &fresh_logs
    )
    .await
    .unwrap());
    ensure_no_pending_group_install(&target_dir).await.unwrap();

    // The fresh node reopens its registry and recovered the committed prefix.
    let reopened =
        ReplicatedStreamRegistry::open(identity(), fresh_raft.clone(), checkpoint_path, adapters())
            .await
            .unwrap();
    let committed = bundle.manifest.state.committed()[&StreamId(1)].clone();
    assert_eq!(
        reopened.refresh().await.unwrap().committed()[&StreamId(1)],
        committed
    );
    assert!(fresh_log.contains(&committed).await.unwrap());

    fresh_raft.stop_async().await.unwrap();
    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}

#[tokio::test]
async fn bundle_cut_pins_the_raft_snapshot_frontier_for_safe_compaction() {
    let c = cluster().await;
    let machine = Arc::new(Machine::default());
    let state = c.registries[0].refresh().await.unwrap();
    let scheduler = StreamApplyScheduler::new(
        &state,
        vec![Arc::new(LoggedStreamAdapter::new(
            c.logs[0].clone(),
            machine.clone(),
        ))],
    )
    .unwrap();
    c.groups[0]
        .append(StreamId(1), 77, b"pin".to_vec(), vec![])
        .await
        .unwrap();
    let state = c.registries[0].refresh().await.unwrap();
    scheduler.apply_stream(&state, StreamId(1)).await.unwrap();

    let bundle = c.groups[0]
        .capture_snapshot_bundle(&scheduler)
        .await
        .unwrap();
    bundle
        .publish(&c.path.join("bundle"), &adapters())
        .await
        .unwrap();
    let frontier = c.groups[0]
        .install_snapshot_frontier(&bundle)
        .await
        .unwrap();
    assert_eq!(frontier, bundle.manifest.state.frontier());

    // A drifted (future) cut is rejected before the Raft snapshot moves, so a
    // conflicting term can never trigger the destructive snapshot-discard path.
    let mut tampered = bundle.clone();
    tampered.manifest.state.cursor.index += 1;
    assert!(c.groups[0]
        .install_snapshot_frontier(&tampered)
        .await
        .is_err());

    // The registry checkpoint at the same cut plus Raft compaction survive a
    // reopen: the bundle's cut is now the authoritative recoverable frontier.
    c.registries[0].checkpoint().await.unwrap();
    c.nodes[0].force_compact_async().await.unwrap();
    let reopened = ReplicatedStreamRegistry::open(
        identity(),
        c.nodes[0].clone(),
        c.path.join("1/registry"),
        adapters(),
    )
    .await
    .unwrap();
    assert_eq!(
        reopened.refresh().await.unwrap().committed()[&StreamId(1)],
        bundle.manifest.state.committed()[&StreamId(1)]
    );
    for node in c.nodes {
        node.stop_async().await.unwrap();
    }
}
