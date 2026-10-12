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
    /// The primary this node is or follows.
    pub primary_node_id: Option<u64>,
    /// Whether this node is the primary.
    pub is_primary: bool,
    /// Whether the primary waits for a replica's durable ack per commit.
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
    /// Synchronous stream commits that failed on the replica-ack timeout.
    pub sync_write_timeouts: u64,
    /// Worker snapshots taken locally.
    pub snapshots_taken: u64,
    /// Worker WAL chunks reclaimed by the prefix GC.
    pub wal_chunks_reclaimed: u64,
    /// Staging chunks reclaimed by the prefix GC.
    pub staging_chunks_reclaimed: u64,
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
    /// Node id of the primary this node is or follows.
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
}

/// Response of the partition-route endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteResponse {
    /// Routed partition/worker pairs.
    pub routes: Vec<PartitionRouteEntry>,
}
