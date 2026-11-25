use mudu::common::id::OID;
use mudu_binding::universal::uni_oid::UniOid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

fn serialize_oid_as_unioid<S>(oid: &OID, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    UniOid::from(*oid).serialize(serializer)
}

fn deserialize_oid_from_unioid<'de, D>(deserializer: D) -> Result<OID, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(UniOid::deserialize(deserializer)?.to_oid())
}

fn serialize_oid_vec_as_unioid<S>(oids: &[OID], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let uni_oids: Vec<UniOid> = oids.iter().copied().map(UniOid::from).collect();
    uni_oids.serialize(serializer)
}

fn deserialize_oid_vec_from_unioid<'de, D>(deserializer: D) -> Result<Vec<OID>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Vec::<UniOid>::deserialize(deserializer)?
        .into_iter()
        .map(|oid| oid.to_oid())
        .collect())
}

/// One worker's entry in the server topology view.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkerTopology {
    /// Worker index.
    pub worker_index: usize,
    /// TCP port this worker listens on.
    pub tcp_listen_port: u16,
    /// Worker id.
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub worker_id: OID,
    /// Partition ids placed on this worker.
    #[serde(
        serialize_with = "serialize_oid_vec_as_unioid",
        deserialize_with = "deserialize_oid_vec_from_unioid"
    )]
    pub partitions: Vec<OID>,
}

/// Topology view of the server: workers, listen ports, and replication.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerTopology {
    /// Number of workers.
    pub worker_count: usize,
    /// Whether each worker listens on its own TCP port.
    pub tcp_multi_port: bool,
    /// Base TCP listen port (worker 0; offset by worker index in multi-port
    /// mode).
    pub tcp_base_listen_port: u16,
    /// Per-worker topology entries.
    pub workers: Vec<WorkerTopology>,
    /// Raft/replication view of this node; absent on a single-node server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replication: Option<ReplicationTopology>,
}

/// Raft/replication view of one cluster node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplicationTopology {
    /// Cluster node id of this server.
    pub node_id: u64,
    /// Current raft role ("Leader", "Follower", "Candidate", "Learner").
    pub role: String,
    /// Current raft term.
    pub term: u64,
    /// Currently known raft leader node id, if any.
    pub leader_node_id: Option<u64>,
    /// Whether this node currently serves client writes (raft leader that
    /// completed leader-elect healing).
    pub serving: bool,
    /// Current raft commit index.
    pub commit_index: u64,
    /// Latest known committed vector: per-worker `(lsn, seg_hash)`.
    pub commit_vec: Vec<CommitVecTopology>,
}

/// One worker's committed point in the topology view.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommitVecTopology {
    /// Worker index.
    pub worker_index: u32,
    /// Highest raft-committed LSN of the worker's stream.
    pub lsn: u64,
    /// Segment hash at `lsn`, lowercase hex.
    pub seg_hash: String,
}

/// Request of the replication membership admin endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplMembershipRequest {
    /// `add_voter` | `add_learner` | `remove` | `promote_learner`.
    pub action: String,
    /// Cluster node id the action applies to.
    pub node_id: u64,
    /// Replication host of an added member (required for add actions).
    #[serde(default)]
    pub host: Option<String>,
    /// Replication port of an added member (required for add actions).
    #[serde(default)]
    pub port: Option<u16>,
}

/// One member in the membership response view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplMembershipMemberView {
    /// Cluster node id.
    pub node_id: u64,
    /// Whether the member votes (false = learner).
    pub can_vote: bool,
    /// Registered replication host, when known.
    pub host: Option<String>,
    /// Registered replication port, when known.
    pub port: Option<u16>,
}

/// Response of the replication membership admin endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplMembershipResponse {
    /// Members of the committed conf.
    pub members: Vec<ReplMembershipMemberView>,
    /// Raft term of the committed conf version.
    pub conf_term: u64,
    /// Committed conf version.
    pub conf_version: u64,
}

/// Response of the replication status endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerReplicationResponse {
    /// `"raft"` | `"stream"` | `"none"` (no replication configured).
    pub mode: String,
    /// This node's cluster id (absent when `mode = "none"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<u64>,
    /// Raft role or `"primary"` / `"replica"` (absent when `mode = "none"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Whether client writes are accepted on this node.
    pub serving: bool,
    /// Raft section (raft mode only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raft: Option<RaftStatusResponse>,
    /// Stream section (stream mode only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<StreamStatusResponse>,
    /// Per-worker replication watermarks.
    #[serde(default)]
    pub workers: Vec<WorkerReplicationResponse>,
    /// Subscriber lags as this node sees them (leader/primary view).
    #[serde(default)]
    pub subscribers: Vec<SubscriberLagResponse>,
    /// Storage usage (periodic cache; zeros until the first refresh).
    pub storage: ReplicationStorageResponse,
    /// Operational counters.
    pub counters: ReplicationCountersResponse,
}

/// Raft section of the replication status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftStatusResponse {
    /// Current raft term.
    pub term: u64,
    /// Currently known leader, if any.
    pub leader_node_id: Option<u64>,
    /// Current commit index.
    pub commit_index: u64,
    /// Latest committed vector: per-worker `(lsn, seg_hash)`.
    pub commit_vec: Vec<CommitVecTopology>,
    /// Voting members of the committed conf.
    pub voters: Vec<u64>,
    /// Learner members of the committed conf.
    pub learners: Vec<u64>,
    /// Committed conf version `(term, version)`.
    pub conf_version_committed: (u64, u64),
    /// New (joint) conf version `(term, version)`; differs while a
    /// re-configuration is in progress.
    pub conf_version_new: (u64, u64),
}

/// Stream section of the replication status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamStatusResponse {
    /// Current stream epoch.
    pub epoch: u64,
    /// The Primary this node is or follows.
    pub primary_node_id: Option<u64>,
    /// Whether this node is the Primary.
    pub is_primary: bool,
    /// Whether the Primary waits for a StreamFollower's durable ack per commit.
    pub sync: bool,
    /// Synchronous replication wait timeout in milliseconds.
    pub sync_timeout_ms: u64,
}

/// One worker's replication watermarks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerReplicationResponse {
    /// Worker index.
    pub worker_index: u32,
    /// Next LSN of the worker's WAL allocator (produced/applied high water).
    pub wal_next_lsn: u64,
    /// Raft committed chain point (raft mode only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_lsn: Option<u64>,
    /// Raft committed chain hash, lowercase hex (raft mode only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_seg_hash: Option<String>,
    /// Highest staged LSN (raft follower staging), when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staging_max_lsn: Option<u64>,
    /// The worker's latest snapshot, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<SnapshotStatusResponse>,
}

/// One worker's latest snapshot metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotStatusResponse {
    /// Committed LSN the snapshot covers.
    pub committed_lsn: u64,
    /// Wall-clock time the snapshot was taken (millis since the epoch).
    pub taken_at_ms: u64,
    /// Snapshot directory size in bytes (from the periodic cache).
    pub dir_bytes: u64,
}

/// One subscriber's replication lag as the leader/primary sees it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriberLagResponse {
    /// Subscriber's cluster node id.
    pub node_id: u64,
    /// The worker this subscription belongs to.
    pub worker_index: u32,
    /// Highest LSN the subscriber reported durably stored.
    pub stored_lsn: u64,
    /// The source's current high-water LSN on that worker.
    pub source_lsn: u64,
    /// `source_lsn - stored_lsn` (LSN/frame units).
    pub lag_lsn: u64,
}

/// Storage usage of the replication state (periodic cache).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationStorageResponse {
    /// Worker WAL chunk bytes (all workers).
    pub wal_bytes: u64,
    /// Staging store bytes (all workers).
    pub staging_bytes: u64,
    /// Raft log bytes.
    pub raft_log_bytes: u64,
    /// Snapshot directory bytes (all workers).
    pub snapshot_bytes: u64,
    /// When the cache was last refreshed (millis since the epoch).
    pub refreshed_at_ms: u64,
}

/// Operational counters of the replication layer (this server's lifetime).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationCountersResponse {
    /// Synchronous stream commits that failed on the StreamFollower ack timeout.
    pub sync_write_timeouts: u64,
    /// Worker snapshots taken locally.
    pub snapshots_taken: u64,
    /// Worker WAL chunks reclaimed by the prefix GC.
    pub wal_chunks_reclaimed: u64,
    /// Staging chunks reclaimed by the prefix GC.
    /// Snapshot offers served to lagging subscribers.
    pub snapshot_offers_served: u64,
    /// Snapshot restores completed on this node.
    pub snapshot_restores_completed: u64,
}

/// Request of the stream promote endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplPromoteRequest {
    /// Operator confirmation that the old primary is fenced (isolated).
    /// The endpoint refuses to promote without it.
    pub fencing_ok: bool,
}

/// Request of the stream follow endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplFollowRequest {
    /// Node id of the primary to follow.
    pub primary_node_id: u64,
    /// Replication host of the primary.
    pub primary_host: String,
    /// Replication port of the primary.
    pub primary_port: u16,
}

/// Response of the stream promote/follow endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplRoleResponse {
    /// This node's role after the operation ("primary" or "replica").
    pub role: String,
    /// The node's current stream epoch.
    pub epoch: u64,
    /// Node id of the Primary this node is or follows.
    pub primary_node_id: u64,
}

/// Request of the partition-route endpoint: a rule name plus either a point
/// key or an inclusive-start/exclusive-end key range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteRequest {
    /// Name of the partition rule to route by.
    pub rule_name: String,
    /// Point-lookup key (mutually exclusive with `start`/`end`).
    #[serde(default)]
    pub key: Option<Vec<String>>,
    /// Range start key, inclusive (mutually exclusive with `key`).
    #[serde(default)]
    pub start: Option<Vec<String>>,
    /// Range end key, exclusive (mutually exclusive with `key`).
    #[serde(default)]
    pub end: Option<Vec<String>>,
    /// Batch of point-lookup keys (mutually exclusive with `key`/`start`/
    /// `end`); the response carries one route group per key, in order.
    #[serde(default)]
    pub keys: Option<Vec<Vec<String>>>,
}

/// One routed partition and the worker hosting it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteEntry {
    /// Partition id.
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub partition_id: mudu::common::id::OID,
    /// Id of the worker the partition is placed on.
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub worker_id: mudu::common::id::OID,
    /// Owning node group (cluster mode only).
    #[serde(default)]
    pub group_id: Option<u64>,
    /// Serving node of the group (cluster mode only).
    #[serde(default)]
    pub node_id: Option<u64>,
    /// Client (pgwire) address of the serving node (cluster mode only), for
    /// direct client routing.
    #[serde(default)]
    pub client_addr: Option<String>,
}

/// Request of the partition-move endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionMoveRequest {
    /// Name of the partition rule.
    pub rule: String,
    /// Name of the partition within the rule.
    pub partition: String,
    /// Target node group.
    pub group_id: u64,
    /// Target worker index within the group's nodes (default 0).
    #[serde(default)]
    pub worker_index: Option<u32>,
}

/// Response of the partition-move endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionMoveResponse {
    /// The partition's route after the move.
    pub routes: Vec<PartitionRouteEntry>,
    /// The placement version the flip committed at (the MetaGroup raft
    /// index), for route-cache invalidation.
    pub placement_version: u64,
    /// Rows re-homed by a split/merge (0 for a move).
    #[serde(default)]
    pub rows_rehomed: u64,
}

/// Request of the partition-split endpoint (phase 3 §9.3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionSplitRequest {
    /// Name of the partition rule.
    pub rule: String,
    /// Name of the partition to split.
    pub partition: String,
    /// Split point: textual bound values, one per partition-key column.
    pub at: Vec<String>,
    /// Name of the new partition receiving the `[at, end)` range.
    pub new_partition: String,
}

/// Request of the partition-merge endpoint (phase 3 §9.3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionMergeRequest {
    /// Name of the partition rule.
    pub rule: String,
    /// Name of the surviving (left) partition.
    pub left: String,
    /// Name of the dropped (right) partition.
    pub right: String,
}

/// Request of the partition-move abort endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionMoveAbortRequest {
    /// Name of the partition rule.
    pub rule: String,
    /// Name of the partition within the rule.
    pub partition: String,
}

/// Response of the partition-route endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteResponse {
    /// Routed partition/worker pairs.
    pub routes: Vec<PartitionRouteEntry>,
    /// Version of the partition rule the routing used.
    #[serde(default)]
    pub rule_version: u64,
    /// Cluster topology version (0 when the server is not in cluster mode).
    #[serde(default)]
    pub topology_version: u64,
    /// Dynamic placement overlay version (phase 3; the raft index of the
    /// last placement-affecting meta op applied on the responding node).
    /// Route caches key on `(topology_version, placement_version)`.
    #[serde(default)]
    pub placement_version: u64,
}

/// Response of the queue observability endpoint (`GET /mudu/server/queues`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerQueuesResponse {
    /// `"ok"` when the per-queue runtime stats were read from the live
    /// workers; `"none"` when the server hosts no queues (an explicit empty
    /// state in the style of the replication endpoint's `mode: "none"`).
    pub mode: String,
    /// Per-queue status entries (empty when `mode` is `"none"`).
    #[serde(default)]
    pub queues: Vec<ServerQueueStatus>,
}

/// Runtime status of one queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerQueueStatus {
    /// Schema (namespace) of the queue.
    pub schema: String,
    /// Queue name.
    pub name: String,
    /// Queue table OID.
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub queue_id: OID,
    /// Index of the worker hosting the queue's runtime.
    pub worker_index: u32,
    /// Messages allocated but not yet acknowledged.
    pub depth: u64,
    /// Encoded payload bytes currently retained.
    pub bytes: u64,
    /// Enqueues rejected by capacity limits since process start.
    pub overflow_count: u64,
    /// Next sequence number to allocate at enqueue (aggregate across shards;
    /// see `shards` for the per-shard values).
    pub next_seq: u64,
    /// Acked frontier: every `__seq` below it is acknowledged and deleted
    /// (aggregate across shards; see `shards` for the per-shard values).
    pub acked_frontier: u64,
    /// Depth of the derived dead-letter queue `<name>_dlq` (absent when the
    /// queue has no DLQ).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dlq_depth: Option<u64>,
    /// Parked `RECEIVE ... WAIT` waiters of this queue.
    pub waiters: u64,
    /// Per-shard runtime counters; one entry for an unpartitioned queue.
    #[serde(default)]
    pub shards: Vec<ServerQueueShardStatus>,
    /// Named consumer cursors of the queue (fan-out mode); empty while the
    /// queue has no consumers (the default competing cursor applies).
    #[serde(default)]
    pub consumers: Vec<ServerQueueConsumer>,
    /// The queue's activation registration (absent when the queue has none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation: Option<ServerQueueActivation>,
}

/// Runtime counters of one queue shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerQueueShardStatus {
    /// Shard index (position in the queue rule's partition list).
    pub shard: u64,
    /// Index of the worker hosting the shard.
    pub worker_index: u32,
    /// Messages allocated but not yet acknowledged on the shard.
    pub depth: u64,
    /// Encoded payload bytes retained by the shard.
    pub bytes: u64,
    /// Next per-shard local sequence to allocate.
    pub next_seq: u64,
    /// Per-shard acked frontier in local sequence units.
    pub acked_frontier: u64,
    /// Parked `RECEIVE ... WAIT` waiters of the shard.
    pub waiters: u64,
}

/// Runtime status of one named consumer cursor of a queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerQueueConsumer {
    /// Consumer cursor name.
    pub name: String,
    /// Shard index the cursor tracks (0 for an unpartitioned queue).
    pub shard: u64,
    /// Cursor frontier: every local `__seq` below it on this shard is
    /// acknowledged by this consumer (a fresh cursor starts at 0).
    pub frontier: u64,
    /// Messages allocated at or past the cursor frontier on this shard
    /// (shard `next_seq` minus `frontier`): the cursor's backlog.
    pub lag: u64,
}

/// Request of the queue activation registration endpoint (`POST
/// /mudu/server/queue_activation`). Registrations are volatile: in-memory
/// only, lost on restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueActivationRequest {
    /// Name of the queue (optionally `schema.name`-qualified); a bare name
    /// must match exactly one queue across the catalog.
    pub queue: String,
    /// Procedure invoked while the queue is non-empty, in
    /// `app/module/procedure` form. The procedure takes exactly one `String`
    /// parameter: the queue name.
    pub procedure: String,
    /// Consecutive invocation failures tolerated before the activation
    /// disables itself (default 5).
    #[serde(default)]
    pub max_retry: Option<u64>,
    /// Batch size hint, stored and reported only (default 32).
    #[serde(default)]
    pub batch: Option<u64>,
}

/// Status of one registered queue activation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerQueueActivation {
    /// Name of the queue.
    pub queue: String,
    /// Registered procedure, in `app/module/procedure` form.
    pub procedure: String,
    /// Whether the activation still fires (false after a failure storm
    /// exceeded `max_retry`).
    pub enabled: bool,
    /// Consecutive invocation failures since the last success.
    pub consecutive_failures: u64,
    /// Consecutive failures tolerated before the activation disables itself.
    pub max_retry: u64,
    /// Stored batch size hint.
    pub batch: u64,
}

/// Response of the queue activation list endpoint (`GET
/// /mudu/server/queue_activations`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerQueueActivationsResponse {
    /// `"ok"` when the per-worker registries were read; `"none"` when no
    /// activation is registered anywhere (an explicit empty state in the
    /// style of the queues endpoint's `mode: "none"`).
    pub mode: String,
    /// Registered activations (empty when `mode` is `"none"`).
    #[serde(default)]
    pub activations: Vec<ServerQueueActivation>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn worker_topology_oid_fields_roundtrip_through_json() {
        let topology = WorkerTopology {
            worker_index: 2,
            tcp_listen_port: 9700,
            worker_id: 0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF,
            partitions: vec![11, 22, 33],
        };
        let json = serde_json::to_string(&topology).unwrap();
        let back: WorkerTopology = serde_json::from_str(&json).unwrap();
        assert_eq!(back.worker_index, 2);
        assert_eq!(back.tcp_listen_port, 9700);
        assert_eq!(back.worker_id, topology.worker_id);
        assert_eq!(back.partitions, vec![11, 22, 33]);
    }

    #[test]
    fn server_topology_with_replication_roundtrips_through_json() {
        let topology = ServerTopology {
            worker_count: 1,
            tcp_multi_port: false,
            tcp_base_listen_port: 9600,
            workers: vec![WorkerTopology {
                worker_index: 0,
                tcp_listen_port: 9600,
                worker_id: 7,
                partitions: vec![70],
            }],
            replication: Some(ReplicationTopology {
                node_id: 1,
                role: "Leader".to_string(),
                term: 3,
                leader_node_id: Some(1),
                serving: true,
                commit_index: 9,
                commit_vec: vec![CommitVecTopology {
                    worker_index: 0,
                    lsn: 42,
                    seg_hash: "ab".to_string(),
                }],
            }),
        };
        let json = serde_json::to_string(&topology).unwrap();
        let back: ServerTopology = serde_json::from_str(&json).unwrap();
        assert_eq!(back.workers.len(), 1);
        assert_eq!(back.workers[0].worker_id, 7);
        let repl = back.replication.expect("replication section");
        assert_eq!(repl.commit_vec[0].lsn, 42);
    }

    #[test]
    fn partition_route_entry_and_queue_status_oid_roundtrip() {
        let entry = PartitionRouteEntry {
            partition_id: 1234,
            worker_id: 5678,
            group_id: Some(2),
            node_id: Some(7),
            client_addr: Some("10.0.0.7:5432".to_string()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: PartitionRouteEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);

        let status = ServerQueueStatus {
            schema: "mududb".to_string(),
            name: "q".to_string(),
            queue_id: 99,
            worker_index: 0,
            depth: 3,
            bytes: 30,
            overflow_count: 1,
            next_seq: 4,
            acked_frontier: 1,
            dlq_depth: Some(0),
            waiters: 2,
            shards: vec![],
            consumers: vec![ServerQueueConsumer {
                name: "c".to_string(),
                shard: 0,
                frontier: 1,
                lag: 2,
            }],
            activation: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        let back: ServerQueueStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back.queue_id, 99);
        assert_eq!(back.consumers.len(), 1);
    }
}
