//! Kernel-side replication configuration types.
//!
//! These are pure data types (no kernel-layer dependencies) shared by the
//! server config and the replication (`repl`) layer.

/// Replication role of a node in a static primary-replica cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplRole {
    /// The primary accepts writes and streams its WAL to followers.
    Primary,
    /// A follower applies the primary's WAL stream and serves reads only.
    Follower,
}

/// A static replication cluster member as seen by the kernel server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplPeerCfg {
    node_id: u64,
    host: String,
    port: u16,
}

impl ReplPeerCfg {
    /// Creates a cluster member descriptor.
    pub fn new(node_id: u64, host: String, port: u16) -> Self {
        Self {
            node_id,
            host,
            port,
        }
    }

    /// Node identifier of the peer.
    pub fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Hostname or IP address the peer's replication channel listens on.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Port the peer's replication channel listens on.
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Kernel-side replication mode, mapped from the runtime configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplMode {
    /// Raft consensus fault tolerance (HA).
    Raft,
    /// Primary-replica streaming replication without raft.
    Stream,
}

/// Kernel-side replication configuration.
///
/// Stream mode: the primary is configured explicitly per node (`primary`);
/// replicas stream from and redirect to it. Raft mode: the leader is
/// elected. When `None` is attached to [`ServerCfg`], the server runs as a
/// single node.
#[derive(Debug, Clone)]
pub struct ReplNodeCfg {
    mode: ReplMode,
    node_id: u64,
    listen_port: u16,
    raft_tick_ms: u64,
    election_timeout_ticks: u64,
    repl_flow_window_segments: u64,
    snapshot_wal_threshold_bytes: u64,
    gc_check_interval_secs: u64,
    primary: Option<bool>,
    sync: Option<bool>,
    sync_timeout_ms: u64,
    peers: Vec<ReplPeerCfg>,
}

impl ReplNodeCfg {
    /// Creates a replication configuration for this node.
    // The fields map 1:1 onto the runtime `ReplicationCfg`; a params struct
    // would only duplicate that definition.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mode: ReplMode,
        node_id: u64,
        listen_port: u16,
        raft_tick_ms: u64,
        election_timeout_ticks: u64,
        repl_flow_window_segments: u64,
        snapshot_wal_threshold_bytes: u64,
        gc_check_interval_secs: u64,
        primary: Option<bool>,
        sync: Option<bool>,
        sync_timeout_ms: u64,
        peers: Vec<ReplPeerCfg>,
    ) -> Self {
        Self {
            mode,
            node_id,
            listen_port,
            raft_tick_ms,
            election_timeout_ticks,
            repl_flow_window_segments,
            snapshot_wal_threshold_bytes,
            gc_check_interval_secs,
            primary,
            sync,
            sync_timeout_ms,
            peers,
        }
    }

    /// The replication mode of this cluster.
    pub fn mode(&self) -> ReplMode {
        self.mode
    }

    /// Whether this node is the configured primary (stream mode).
    pub fn primary(&self) -> Option<bool> {
        self.primary
    }

    /// Whether the primary waits for a replica's durable ack before
    /// answering a commit (stream mode).
    pub fn sync(&self) -> Option<bool> {
        self.sync
    }

    /// Synchronous replication wait timeout in milliseconds.
    pub fn sync_timeout_ms(&self) -> u64 {
        self.sync_timeout_ms
    }

    /// Identifier of this node.
    pub fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Port this node's replication channel listens on.
    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    /// Raft state machine tick interval in milliseconds.
    pub fn raft_tick_ms(&self) -> u64 {
        self.raft_tick_ms
    }

    /// Raft election timeout in ticks.
    pub fn election_timeout_ticks(&self) -> u64 {
        self.election_timeout_ticks
    }

    /// Flow-control window of the replication stream, in segments (batches).
    pub fn repl_flow_window_segments(&self) -> u64 {
        self.repl_flow_window_segments
    }

    /// Committed WAL bytes beyond the last snapshot that trigger the next
    /// per-worker snapshot.
    pub fn snapshot_wal_threshold_bytes(&self) -> u64 {
        self.snapshot_wal_threshold_bytes
    }

    /// Interval between WAL/staging GC watermark evaluations, in seconds.
    pub fn gc_check_interval_secs(&self) -> u64 {
        self.gc_check_interval_secs
    }

    /// Static cluster members, including this node.
    pub fn peers(&self) -> &[ReplPeerCfg] {
        &self.peers
    }

    /// The designated primary: the cluster member with the smallest node id.
    pub(crate) fn primary_peer(&self) -> Option<&ReplPeerCfg> {
        self.peers.iter().min_by_key(|peer| peer.node_id())
    }

    /// The configured primary's node id for stream mode: the non-self peer
    /// in a two-node cluster, else the smallest node id (mirrors
    /// [`Self::configured_primary_host`]).
    pub fn configured_primary_node_id(&self) -> Option<u64> {
        if self.peers.len() == 2 {
            return self
                .peers
                .iter()
                .find(|peer| peer.node_id() != self.node_id)
                .map(|peer| peer.node_id());
        }
        self.primary_peer().map(|peer| peer.node_id())
    }

    /// The configured primary's host for stream mode: the non-self
    /// peer in a two-node cluster, else the smallest node id (the static
    /// tie-break). Replicas stream from and redirect clients to this node.
    pub fn configured_primary_host(&self) -> Option<String> {
        if self.peers.len() == 2 {
            return self
                .peers
                .iter()
                .find(|peer| peer.node_id() != self.node_id)
                .map(|peer| peer.host().to_string());
        }
        self.primary_peer().map(|peer| peer.host().to_string())
    }

    /// The configured primary's replication address (`host:repl_port`).
    pub fn configured_primary_repl_addr(&self) -> Option<String> {
        if self.peers.len() == 2 {
            return self
                .peers
                .iter()
                .find(|peer| peer.node_id() != self.node_id)
                .map(|peer| format!("{}:{}", peer.host(), peer.port()));
        }
        self.primary_addr()
    }

    /// Returns true when this node is the designated primary.
    pub fn is_primary(&self) -> bool {
        self.primary_peer()
            .is_some_and(|peer| peer.node_id() == self.node_id)
    }

    /// The replication role derived from the static cluster membership.
    pub fn role(&self) -> ReplRole {
        if self.is_primary() {
            ReplRole::Primary
        } else {
            ReplRole::Follower
        }
    }

    /// `host:port` string of the designated primary's replication channel.
    pub fn primary_addr(&self) -> Option<String> {
        self.primary_peer()
            .map(|peer| format!("{}:{}", peer.host(), peer.port()))
    }

    /// `host:port` string of the replication channel of the given node id.
    pub fn peer_addr(&self, node_id: u64) -> Option<String> {
        self.peers
            .iter()
            .find(|peer| peer.node_id() == node_id)
            .map(|peer| format!("{}:{}", peer.host(), peer.port()))
    }
}
