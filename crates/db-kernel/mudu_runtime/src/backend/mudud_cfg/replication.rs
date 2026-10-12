use super::{
    default_election_timeout_ticks, default_gc_check_interval_secs, default_raft_tick_ms,
    default_repl_flow_window_segments, default_snapshot_wal_threshold_bytes,
    default_sync_timeout_ms,
};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// TCP connection routing strategy.
#[derive(Eq, PartialEq, Debug, Clone, Copy, Default)]
pub enum RoutingMode {
    /// Route by connection identifier.
    #[default]
    ConnectionId,
    /// Route by player identifier.
    PlayerId,
    /// Route by remote hash.
    RemoteHash,
}

impl Serialize for RoutingMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:?}", self))
    }
}

impl<'de> Deserialize<'de> for RoutingMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_ascii_lowercase().as_str() {
            "connectionid" | "connection_id" => Ok(RoutingMode::ConnectionId),
            "playerid" | "player_id" => Ok(RoutingMode::PlayerId),
            "remotehash" | "remote_hash" => Ok(RoutingMode::RemoteHash),
            other => Err(serde::de::Error::custom(format!(
                "unknown routing mode: {}",
                other
            ))),
        }
    }
}

/// Replication cluster mode, derived from which `[replication.*]`
/// subsection is present. Never serialized directly.
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum ReplicationMode {
    /// Raft consensus fault tolerance (HA): majority commit gating,
    /// CommitVec consensus entries and automatic leader election.
    Raft,
    /// Primary-replica streaming replication without raft: a configured
    /// primary, manual failover, optional synchronous replication.
    Stream,
}

/// A static replication cluster member.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ReplicationPeer {
    /// Node identifier of the peer.
    pub node_id: u64,
    /// Hostname or IP address the peer's replication channel listens on.
    pub host: String,
    /// Port the peer's replication channel listens on.
    pub port: u16,
}

/// `[replication.stream]` subsection: primary-replica streaming replication
/// without raft. `primary` and `sync` are structurally required (a missing
/// key fails TOML parsing), unlike the earlier flat `mode = "stream"` shape.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct StreamCfg {
    /// Whether this node is the configured primary; exactly one cluster
    /// member sets `true`.
    pub primary: bool,
    /// Whether the primary waits for a replica's durable ack before
    /// answering a commit.
    pub sync: bool,
    /// Synchronous replication wait timeout in milliseconds.
    #[serde(default = "default_sync_timeout_ms")]
    pub sync_timeout_ms: u64,
}

/// `[replication.raft]` subsection: Raft consensus fault tolerance. All
/// knobs are optional.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct RaftReplCfg {
    /// Raft state machine tick interval in milliseconds.
    #[serde(default = "default_raft_tick_ms")]
    pub raft_tick_ms: u64,
    /// Raft election timeout in ticks.
    #[serde(default = "default_election_timeout_ticks")]
    pub election_timeout_ticks: u64,
}

/// Primary-replica streaming replication and Raft fault-tolerance configuration.
///
/// When this section is absent the server runs as a single node: no
/// replication or consensus components are started and behavior is identical
/// to a standalone deployment. Exactly one of the `stream` / `raft`
/// subsections must be present — the present subsection selects the mode.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ReplicationCfg {
    /// Identifier of this node. Persisted to `<db_path>/node_id` on first
    /// boot; a mismatch with the persisted value is a startup error.
    pub node_id: u64,
    /// Port the replication and Raft channel listens on. Replication streams
    /// and Raft messages are multiplexed over this single port by message type.
    pub listen_port: u16,
    /// Flow-control window of the replication stream, in segments.
    #[serde(default = "default_repl_flow_window_segments")]
    pub repl_flow_window_segments: u64,
    /// Committed WAL bytes beyond the last snapshot that trigger the next
    /// per-worker snapshot (and with it the WAL/raft-log GC frontier).
    #[serde(default = "default_snapshot_wal_threshold_bytes")]
    pub snapshot_wal_threshold_bytes: u64,
    /// Interval between WAL/staging GC watermark evaluations, in seconds.
    #[serde(default = "default_gc_check_interval_secs")]
    pub gc_check_interval_secs: u64,
    /// `[replication.stream]`: primary-replica streaming replication mode.
    #[serde(default)]
    pub stream: Option<StreamCfg>,
    /// `[replication.raft]`: Raft consensus fault-tolerance mode.
    #[serde(default)]
    pub raft: Option<RaftReplCfg>,
    /// Static cluster members; must contain this node's `node_id`.
    #[serde(default)]
    pub peers: Vec<ReplicationPeer>,
}

impl ReplicationCfg {
    /// The mode selected by the present subsection: exactly one of
    /// `stream` / `raft` must be set. Both absent and both present are
    /// configuration errors.
    pub fn mode(&self) -> RS<ReplicationMode> {
        match (&self.stream, &self.raft) {
            (Some(_), None) => Ok(ReplicationMode::Stream),
            (None, Some(_)) => Ok(ReplicationMode::Raft),
            (None, None) => Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "[replication] requires exactly one of [replication.stream] / [replication.raft]"
            )),
            (Some(_), Some(_)) => Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "[replication.stream] and [replication.raft] are mutually exclusive"
            )),
        }
    }

    /// Validates the snapshot/GC knobs shared by both modes.
    pub fn validate_snapshot_gc(&self) -> RS<()> {
        if self.snapshot_wal_threshold_bytes == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "replication.snapshot_wal_threshold_bytes must be greater than zero"
            ));
        }
        if self.gc_check_interval_secs == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "replication.gc_check_interval_secs must be greater than zero"
            ));
        }
        Ok(())
    }
}

/// Backup / point-in-time-recovery configuration.
///
/// When this section is absent the server runs backup-free: WAL prefix GC
/// reclaims chunks without archiving them. When present, sealed WAL chunks are
/// continuously archived to `archive_dir` before GC may reclaim them.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, Default)]
pub struct BackupCfg {
    /// Directory WAL chunks are archived to (continuous archiving).
    #[serde(default)]
    pub archive_dir: Option<String>,
    /// Directory base backups are written to (`mcli backup`).
    #[serde(default)]
    pub backup_dir: Option<String>,
    /// Archiving mode: `"off"`, `"on"`, or `"always"`.
    #[serde(default = "default_archive_mode")]
    pub archive_mode: String,
}

impl BackupCfg {
    /// True when continuous WAL archiving is enabled.
    pub fn archiving_enabled(&self) -> bool {
        self.archive_dir.is_some() && self.archive_mode != "off"
    }
}

fn default_archive_mode() -> String {
    "on".to_string()
}
