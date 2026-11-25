use super::{default_election_timeout_ticks, default_raft_tick_ms, default_sync_timeout_ms};
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

/// `replica.raft` parameters. All knobs are optional.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct RaftReplCfg {
    /// Raft state machine tick interval in milliseconds.
    #[serde(default = "default_raft_tick_ms")]
    pub tick_ms: u64,
    /// Raft election timeout in ticks.
    #[serde(default = "default_election_timeout_ticks")]
    pub election_timeout_ticks: u64,
}

/// `replica.stream` parameters. `bootstrap_primary` and `sync` are
/// structurally required (a missing key fails TOML parsing).
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct StreamCfg {
    /// Name of the initial primary; must reference a member of the same group
    /// and is used on first boot only.
    pub bootstrap_primary: String,
    /// Whether the Primary waits for a StreamFollower's durable ack before
    /// answering a commit.
    pub sync: bool,
    /// Synchronous replication wait timeout in milliseconds.
    #[serde(default = "default_sync_timeout_ms")]
    pub sync_timeout_ms: u64,
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
