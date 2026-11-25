//! One shared replication group carrying every control and data stream: a single
//! registry, a single
//! `GenericReplicationGroup` and the durable logs of all streams. The control
//! and data services attach their business machines and drivers to this shared
//! owner instead of each running its own group.

use super::{
    AdapterCapabilities, DurableStreamLog, GenericReplicationGroup, ReplicatedStreamRegistry,
    StreamTransport,
};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{GroupIdobject, RegistryOp, StreamDescriptor, StreamId};
use mudu_raft::RaftHandle;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(60);
const REGISTRATION_POLL: Duration = Duration::from_millis(50);

/// A shared replication group: one registry, one group and the durable log of
/// every registered stream.
pub struct SharedGroup {
    /// The single Raft handle of the group.
    pub raft: Arc<RaftHandle>,
    /// The single stream registry of the group.
    pub registry: Arc<ReplicatedStreamRegistry>,
    /// The single replication group.
    pub group: Arc<GenericReplicationGroup>,
    /// One durable external log per registered stream.
    pub logs: BTreeMap<StreamId, Arc<DurableStreamLog>>,
}

impl SharedGroup {
    /// Opens one registry + group carrying every given stream, registering each
    /// descriptor under its own stream-id-derived request id so the control and
    /// data bootstrap request-id spaces cannot collide. `bootstrap_stop`
    /// aborts the registration wait promptly on server shutdown.
    pub async fn open(
        raft: Arc<RaftHandle>,
        identity: GroupIdobject,
        capabilities: AdapterCapabilities,
        descriptors: Vec<StreamDescriptor>,
        log_dir: &Path,
        transport: Arc<dyn StreamTransport>,
        bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
    ) -> RS<Self> {
        // The registry checkpoint's parent must exist before it opens.
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::ensure_dir(fs.as_ref(), log_dir).await?;
        let registry = Arc::new(
            ReplicatedStreamRegistry::open(
                identity,
                raft.clone(),
                log_dir.join("registry.checkpoint"),
                capabilities,
            )
            .await?,
        );
        register_streams(&registry, &descriptors, bootstrap_stop).await?;
        let mut logs = BTreeMap::new();
        for descriptor in &descriptors {
            logs.insert(
                descriptor.stream_id,
                Arc::new(
                    DurableStreamLog::open(
                        log_dir.join(format!("stream-{}", descriptor.stream_id.0)),
                        identity,
                        descriptor.clone(),
                    )
                    .await?,
                ),
            );
        }
        let group = Arc::new(
            GenericReplicationGroup::open(
                raft.clone(),
                registry.clone(),
                logs.values().cloned().collect(),
                transport,
            )
            .await?,
        );
        Ok(Self {
            raft,
            registry,
            group,
            logs,
        })
    }
}

/// Registers every descriptor, idempotently and concurrently across nodes: any
/// node that becomes leader commits the missing registrations and followers
/// converge through the replicated registry. `bootstrap_stop` aborts the wait
/// promptly on server shutdown.
async fn register_streams(
    registry: &Arc<ReplicatedStreamRegistry>,
    descriptors: &[StreamDescriptor],
    mut bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
) -> RS<()> {
    let wait = async {
        loop {
            let state = registry.refresh().await?;
            let missing: Vec<StreamDescriptor> = descriptors
                .iter()
                .filter(|descriptor| !state.streams().contains_key(&descriptor.stream_id))
                .cloned()
                .collect();
            if missing.is_empty() {
                return Ok(());
            }
            if registry.leader_view().await.is_ok() {
                for descriptor in &missing {
                    let request_id = u128::from(descriptor.stream_id.0);
                    let _ = registry
                        .change(request_id, RegistryOp::Register(descriptor.clone()))
                        .await;
                }
            }
            if let Some(stop) = &mut bootstrap_stop {
                mudu_sys::tokio::select! {
                    _ = mudu_sys::task::async_::sleep(REGISTRATION_POLL) => {}
                    _ = stop.changed() => {
                        return Err(mudu_error!(
                            ErrorCode::Interrupted,
                            "shared stream registration aborted by shutdown"
                        ));
                    }
                }
            } else {
                mudu_sys::task::async_::sleep(REGISTRATION_POLL).await?;
            }
        }
    };
    mudu_sys::timeout(REGISTRATION_TIMEOUT, wait)
        .await
        .ok_or_else(|| mudu_error!(ErrorCode::TimedOut, "shared stream registration timed out"))?
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::tests::{adapters, descriptor, identity, start_node, wait_leader};
    use super::*;
    use crate::log_stream::{AckRound, DurableAck, StreamFetch, StreamSession};
    use async_trait::async_trait;
    use mudu::common::result::RS;
    use mudu_raft::stream::{StreamId, StreamProgress};
    use std::sync::Arc;

    /// A transport that never reaches a peer: sufficient for a single node.
    struct NoPeers;
    #[async_trait]
    impl StreamTransport for NoPeers {
        async fn session(&self, _: u64) -> RS<StreamSession> {
            Err(super::super::state::invalid("unexpected peer"))
        }
        async fn push(
            &self,
            _: u64,
            _: &StreamSession,
            _: &AckRound,
            _: Vec<u8>,
        ) -> RS<Option<DurableAck>> {
            Err(super::super::state::invalid("unexpected peer"))
        }
        async fn fetch(&self, _: u64, _: &StreamSession, _: StreamFetch) -> RS<Vec<u8>> {
            Err(super::super::state::invalid("unexpected peer"))
        }
        async fn progress_at(
            &self,
            _: u64,
            _: &StreamSession,
            _: StreamId,
            _: u64,
        ) -> RS<Option<StreamProgress>> {
            Err(super::super::state::invalid("unexpected peer"))
        }
    }

    #[test]
    fn shared_group_carries_control_and_data_streams_on_one_raft() {
        let path = mudu_sys::env_var::temp_dir()
            .join(format!("shared-group-{}", mudu_sys::random::uuid_v4()));
        let raft = start_node(&path);
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            wait_leader(&raft).await;
            // Three stream ids that mirror the merged control/data layout:
            // 1 = meta, 2 = epoch, 3 = tx_decision.
            let descriptors = vec![descriptor(1), descriptor(2), descriptor(3)];
            let shared = SharedGroup::open(
                raft.clone(),
                identity(),
                adapters(),
                descriptors,
                &path,
                Arc::new(NoPeers),
                None,
            )
            .await
            .unwrap();
            assert_eq!(shared.logs.len(), 3);
            // Every stream commits its own prefix on the single group.
            for id in [1u64, 2, 3] {
                let receipt = shared
                    .group
                    .append(StreamId(id), u128::from(id), vec![id as u8], vec![])
                    .await
                    .unwrap();
                assert_eq!(receipt.progress.stream_id, StreamId(id));
            }
            let state = shared.registry.refresh().await.unwrap();
            assert_eq!(state.streams().len(), 3);
            assert!(state.committed().contains_key(&StreamId(1)));
            assert!(state.committed().contains_key(&StreamId(2)));
            assert!(state.committed().contains_key(&StreamId(3)));
            shared.group.shutdown().await.unwrap();
        });
        drop(raft);
        mudu_sys::fs::sync::remove_dir_all(&path).unwrap();
    }
}
