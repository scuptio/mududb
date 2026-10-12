use crate::backend::cfg_meta::ConfigMutability;
use ::wal::worker_log::WalSyncPolicy;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use runtime_wasm::service::runtime_opt::ComponentTarget;
pub use runtime_wasm::service::runtime_opt::ServerMode;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::time::Duration;

mod io;
mod replication;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use io::load_mudud_cfg_with_local;
pub use io::{
    NODE_ID_FILE_NAME, default_cfg_path, init_mudud_cfg, load_mudud_cfg, load_or_create_node_id,
};
pub use replication::{
    BackupCfg, RaftReplCfg, ReplicationCfg, ReplicationMode, ReplicationPeer, RoutingMode,
    StreamCfg,
};

/// MuduDB server configuration.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct MuduDBCfg {
    /// Procedure budgets; changes apply to newly constructed runtimes.
    #[serde(default)]
    pub invocation_limits: runtime_wasm::service::invocation_limits::InvocationLimits,
    /// Path to the application package.
    pub mpk_path: String,
    /// Path to the database directory.
    pub db_path: String,
    /// IP address to listen on.
    pub listen_ip: String,
    /// HTTP API listening port.
    pub http_listen_port: u16,
    /// Number of threads used by the HTTP worker pool.
    #[serde(default = "default_http_worker_threads")]
    pub http_worker_threads: usize,
    /// Postgres wire protocol listening port.
    pub pg_listen_port: u16,
    /// Target Wasm component model version.
    #[serde(default)]
    pub component_target: Option<ComponentTarget>,
    /// Whether async runtime support is enabled.
    pub enable_async: bool,
    /// Selected server execution mode.
    #[serde(default)]
    pub server_mode: ServerMode,
    /// TCP listening port.
    #[serde(default = "default_tcp_listen_port")]
    pub tcp_listen_port: u16,
    /// Whether workers listen on multiple consecutive ports.
    #[serde(default)]
    pub tcp_multi_port: bool,
    /// Number of worker threads.
    #[serde(default)]
    pub worker_threads: usize,
    /// io_uring completion queue ring entries.
    #[serde(default = "default_ring_entries")]
    pub io_uring_ring_entries: u32,
    /// Enable io_uring accept multishot.
    #[serde(default = "default_true")]
    pub io_uring_accept_multishot: bool,
    /// Enable io_uring receive multishot.
    #[serde(default = "default_true")]
    pub io_uring_recv_multishot: bool,
    /// Enable io_uring fixed buffers.
    #[serde(default)]
    pub io_uring_enable_fixed_buffers: bool,
    /// Enable io_uring fixed files.
    #[serde(default)]
    pub io_uring_enable_fixed_files: bool,
    /// TCP routing mode.
    #[serde(default)]
    pub routing_mode: RoutingMode,
    /// WAL log chunk size in bytes.
    #[serde(default = "default_log_chunk_size")]
    pub log_chunk_size: u64,
    /// Database page size in bytes.
    ///
    /// This is a `ConfigMutability::Persistent` setting: it is written into the
    /// on-disk format of page files. Changing it for an existing database
    /// requires a migration tool that rewrites all data files.
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    /// Group-commit batching window in microseconds: how long a queued WAL
    /// commit batch may age before the flush driver flushes it. Larger
    /// values improve fsync sharing under concurrency; smaller values lower
    /// lone-commit latency. Defaults to 200 when omitted.
    #[serde(default = "default_wal_flush_max_wait_us")]
    pub wal_flush_max_wait_us: u64,
    /// WAL durability mode: "commit" fsyncs every flush round before the
    /// commit is acknowledged; "periodic" acknowledges once the WAL write
    /// lands in the page cache and fsyncs at most once per
    /// `wal_sync_interval_ms`.
    #[serde(default = "default_wal_sync_mode")]
    pub wal_sync_mode: String,
    /// WAL fsync interval in milliseconds for `wal_sync_mode = "periodic"`.
    /// A power loss may lose acknowledged commits from the last interval.
    #[serde(default = "default_wal_sync_interval_ms")]
    pub wal_sync_interval_ms: u64,
    /// Primary-replica replication and Raft fault tolerance. Absent means
    /// single-node mode with no replication components started.
    #[serde(default)]
    pub replication: Option<ReplicationCfg>,
    /// Backup and point-in-time recovery. Absent means backup-free.
    #[serde(default)]
    pub backup: Option<BackupCfg>,
}

impl Display for MuduDBCfg {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let component_target = self.component_target();
        writeln!(f, "MuduDB Setting:")?;
        writeln!(f, "-------------------")?;
        writeln!(f, "  -> Package path: {}", self.mpk_path)?;
        writeln!(f, "  -> Data path: {}", self.db_path)?;
        writeln!(f, "  -> Listen IP address: {}", self.listen_ip)?;
        writeln!(f, "  -> HTTP Listening port: {}", self.http_listen_port)?;
        writeln!(f, "  -> HTTP worker threads: {}", self.http_worker_threads)?;
        writeln!(f, "  -> PG Listening port: {}", self.pg_listen_port)?;
        writeln!(f, "  -> Component target: {:?}", component_target)?;
        writeln!(f, "  -> Enable Async: {}", self.enable_async)?;
        writeln!(f, "  -> Server mode: {:?}", self.server_mode)?;
        writeln!(f, "  -> TCP Listening port: {}", self.tcp_listen_port)?;
        writeln!(f, "  -> TCP Multi-port: {}", self.tcp_multi_port)?;
        writeln!(f, "  -> workers: {}", self.worker_threads)?;
        writeln!(
            f,
            "  -> io_uring ring entries: {}",
            self.io_uring_ring_entries
        )?;
        writeln!(
            f,
            "  -> io_uring accept multishot: {}",
            self.io_uring_accept_multishot
        )?;
        writeln!(
            f,
            "  -> io_uring recv multishot: {}",
            self.io_uring_recv_multishot
        )?;
        writeln!(
            f,
            "  -> io_uring fixed buffers: {}",
            self.io_uring_enable_fixed_buffers
        )?;
        writeln!(
            f,
            "  -> io_uring fixed files: {}",
            self.io_uring_enable_fixed_files
        )?;
        writeln!(f, "  -> Routing mode: {:?}", self.routing_mode)?;
        writeln!(f, "  -> log chunk size: {}", self.log_chunk_size)?;
        writeln!(f, "  -> page size: {}", self.page_size)?;
        writeln!(f, "  -> WAL sync mode: {}", self.wal_sync_mode)?;
        writeln!(
            f,
            "  -> WAL sync interval ms: {}",
            self.wal_sync_interval_ms
        )?;
        match &self.replication {
            Some(replication) => writeln!(
                f,
                "  -> Replication: node {} on port {}, {} peer(s)",
                replication.node_id,
                replication.listen_port,
                replication.peers.len()
            )?,
            None => writeln!(f, "  -> Replication: disabled (single node)")?,
        }
        writeln!(f, "-------------------")?;
        Ok(())
    }
}

impl Default for MuduDBCfg {
    fn default() -> Self {
        Self {
            invocation_limits: Default::default(),
            mpk_path: mudu_sys::env_var::temp_dir().to_string_lossy().to_string(),
            db_path: mudu_sys::env_var::temp_dir().to_string_lossy().to_string(),
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: 8300,
            http_worker_threads: default_http_worker_threads(),
            pg_listen_port: 5432,
            component_target: None,
            enable_async: true,
            server_mode: ServerMode::Tokio,
            tcp_listen_port: default_tcp_listen_port(),
            tcp_multi_port: false,
            worker_threads: 0,
            io_uring_ring_entries: default_ring_entries(),
            io_uring_accept_multishot: true,
            io_uring_recv_multishot: true,
            io_uring_enable_fixed_buffers: false,
            io_uring_enable_fixed_files: false,
            routing_mode: RoutingMode::ConnectionId,
            log_chunk_size: default_log_chunk_size(),
            page_size: default_page_size(),
            wal_flush_max_wait_us: default_wal_flush_max_wait_us(),
            wal_sync_mode: default_wal_sync_mode(),
            wal_sync_interval_ms: default_wal_sync_interval_ms(),
            replication: None,
            backup: None,
        }
    }
}

const MUDUDB_CFG_FILE_NAME: &str = "mudud.cfg";
const MUDUDB_CONFIG_DIR: &str = ".mududb";

const DEFAULT_CFG_TEMPLATE: &str = r#"# MuduDB server configuration
# Generated by mudud. Edit as needed.

# Directory containing .mpk application packages.
mpk_path = "./mpk"

# Directory for database storage files.
db_path = "./data"

# IP address to listen on.
listen_ip = "127.0.0.1"

# HTTP management API port.
http_listen_port = 8300

# Number of HTTP worker threads.
http_worker_threads = 1

# PostgreSQL wire protocol port.
pg_listen_port = 5432

# Internal TCP port used by io_uring workers.
tcp_listen_port = 9527

# Server execution mode: "Legacy", "IOUring", or "Tokio".
server_mode = "Tokio"

# Number of worker threads. 0 means auto-detect CPU cores.
worker_threads = 0

# io_uring completion queue ring entries.
io_uring_ring_entries = 1024

# Enable io_uring accept/receive multishot optimizations.
io_uring_accept_multishot = true
io_uring_recv_multishot = true

# Enable fixed buffers/files for io_uring (experimental).
io_uring_enable_fixed_buffers = false
io_uring_enable_fixed_files = false

# TCP routing mode: "ConnectionId", "PlayerId", or "RemoteHash".
routing_mode = "ConnectionId"

# Async runtime support.
enable_async = true

# Use multiple consecutive TCP ports for workers.
tcp_multi_port = false

# WAL log chunk size in bytes.
log_chunk_size = 67108864

# Database page size in bytes. Persistent: changing it requires re-initialization.
page_size = 4096

# WAL durability mode: "commit" or "periodic".
# "commit" fsyncs the WAL before acknowledging every commit.
# "periodic" acknowledges once the WAL write lands in the OS page cache and
# fsyncs at most once per wal_sync_interval_ms; a power loss may lose
# acknowledged commits from the last interval.
# WAL 持久化模式："commit" 或 "periodic"。periodic 模式下掉电可能丢失最近
# wal_sync_interval_ms 毫秒内已确认的提交。
wal_sync_mode = "commit"

# WAL fsync interval in milliseconds for wal_sync_mode = "periodic".
# periodic 模式下的 WAL fsync 间隔（毫秒）。
wal_sync_interval_ms = 10

# Primary-replica streaming replication and Raft fault tolerance.
# Uncomment to enable; when absent the server runs as a single node.
# Exactly one of [replication.stream] / [replication.raft] must be present;
# the present subsection selects the mode.
# 主从流复制与 Raft 容错配置。取消注释以启用；缺省时为单机模式。
# [replication.stream] 与 [replication.raft] 二选一，存在的子段即模式。
# [replication]
# node_id = 1
# listen_port = 19527
# repl_flow_window_segments = 64
# snapshot_wal_threshold_bytes = 1073741824   # optional: committed WAL bytes past the last snapshot that trigger the next snapshot
# gc_check_interval_secs = 60                 # optional: WAL/staging GC evaluation interval
#
# Mode 1: primary-replica streaming replication (no raft).
# 模式一：主从流复制（无 raft）。
# [replication.stream]
# primary = true                 # required: whether this node is the primary; exactly one in the cluster
# sync = true                    # required: true = wait for a replica's durable ack before answering a commit
# sync_timeout_ms = 10000        # optional: synchronous replication wait timeout
#
# Mode 2: Raft consensus fault tolerance (HA).
# 模式二：Raft 共识容错（HA）。
# [replication.raft]
# raft_tick_ms = 50              # optional: raft state machine tick interval
# election_timeout_ticks = 500   # optional: election timeout in ticks
#
# [[replication.peers]]
# node_id = 1
# host = "127.0.0.1"
# port = 19527
#
# [[replication.peers]]
# node_id = 2
# host = "127.0.0.1"
# port = 19527
#
# [[replication.peers]]
# node_id = 3
# host = "127.0.0.1"
# port = 19527
"#;

impl MuduDBCfg {
    /// Returns the effective component target, defaulting to P2.
    pub fn component_target(&self) -> ComponentTarget {
        self.component_target.unwrap_or(ComponentTarget::P2)
    }

    /// Returns true when the selected server mode uses the MuduDB kernel.
    pub fn uses_mududb_kernel(&self) -> bool {
        matches!(self.server_mode, ServerMode::IOUring | ServerMode::Tokio)
    }

    /// Returns the configured worker thread count, falling back to available parallelism.
    pub fn effective_worker_threads(&self) -> usize {
        if self.worker_threads > 0 {
            self.worker_threads
        } else {
            std::thread::available_parallelism()
                .map(|v| v.get())
                .unwrap_or(1)
        }
    }

    /// Returns the mutability class of a known configuration field.
    ///
    /// Unknown field names return `ConfigMutability::RestartRequired` as a
    /// conservative default.
    pub fn mutability_of(field_name: &str) -> ConfigMutability {
        match field_name {
            // Persistent: changing these for an existing database requires data
            // migration or re-initialization.
            "db_path" | "page_size" => ConfigMutability::Persistent,

            // Runtime: derived at runtime or intended for hot-reload.
            "tcp_multi_port" => ConfigMutability::Runtime,

            // Everything else requires a process restart to take effect.
            _ => ConfigMutability::RestartRequired,
        }
    }

    /// Maps `wal_sync_mode` / `wal_sync_interval_ms` to the kernel's WAL
    /// durability policy. Unknown mode strings are rejected with a clear
    /// startup error.
    pub fn wal_sync_policy(&self) -> RS<WalSyncPolicy> {
        match self.wal_sync_mode.to_ascii_lowercase().as_str() {
            "commit" => Ok(WalSyncPolicy::Commit),
            "periodic" => Ok(WalSyncPolicy::Periodic {
                interval: Duration::from_millis(self.wal_sync_interval_ms),
            }),
            other => Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "unknown wal_sync_mode: \"{}\", expected \"commit\" or \"periodic\"",
                    other
                )
            )),
        }
    }

    /// Validates the replication configuration when present: the peer list
    /// must be non-empty with unique node ids and addresses, must contain this
    /// node's `node_id`, and the replication port must not collide with the
    /// client-facing ports. Exactly one of the `[replication.stream]` /
    /// `[replication.raft]` subsections must be present and selects the mode:
    /// a two-peer raft list only warns (it is the sanctioned bootstrap shape
    /// of a joining node, but discouraged as a steady-state cluster); stream
    /// requires at least two peers. Returns `Ok(())` for single-node
    /// configurations.
    pub fn validate_replication(&self) -> RS<()> {
        let Some(replication) = &self.replication else {
            return Ok(());
        };
        replication.validate_snapshot_gc()?;
        if replication.peers.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "replication.peers must not be empty"
            ));
        }
        let mut seen_ids = std::collections::HashSet::new();
        let mut seen_addrs = std::collections::HashSet::new();
        let mut contains_self = false;
        for peer in &replication.peers {
            if !seen_ids.insert(peer.node_id) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("duplicate replication peer node_id {}", peer.node_id)
                ));
            }
            if !seen_addrs.insert((peer.host.as_str(), peer.port)) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "duplicate replication peer address {}:{}",
                        peer.host, peer.port
                    )
                ));
            }
            if peer.node_id == replication.node_id {
                contains_self = true;
            }
        }
        if !contains_self {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "replication.peers does not contain this node's node_id {}",
                    replication.node_id
                )
            ));
        }
        match replication.mode()? {
            ReplicationMode::Raft => {
                if replication.peers.len() == 2 {
                    // A two-entry peer list is the sanctioned bootstrap
                    // shape of a JOINING node (itself + one existing
                    // member; the stored/leader conf wins). As a
                    // steady-state deployment a two-node raft group is
                    // still discouraged: no automatic fault tolerance and
                    // worse write availability than a single node.
                    tracing::warn!(
                        "[replication.raft] with two peers is only recommended for bootstrapping a joining node, not as a steady-state cluster"
                    );
                }
            }
            ReplicationMode::Stream => {
                if replication.peers.len() < 2 {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        "[replication.stream] requires at least two peers (a primary and a replica)"
                    ));
                }
            }
        }
        for (name, port) in [
            ("http_listen_port", self.http_listen_port),
            ("pg_listen_port", self.pg_listen_port),
            ("tcp_listen_port", self.tcp_listen_port),
        ] {
            if replication.listen_port == port {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("replication.listen_port {} collides with {}", port, name)
                ));
            }
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

fn default_http_worker_threads() -> usize {
    1
}

fn default_tcp_listen_port() -> u16 {
    9527
}

fn default_ring_entries() -> u32 {
    1024
}

fn default_log_chunk_size() -> u64 {
    64 * 1024 * 1024
}

fn default_page_size() -> usize {
    4096
}

fn default_wal_flush_max_wait_us() -> u64 {
    200
}

fn default_wal_sync_mode() -> String {
    "commit".to_string()
}

fn default_wal_sync_interval_ms() -> u64 {
    10
}

pub(crate) fn default_raft_tick_ms() -> u64 {
    50
}

pub(crate) fn default_election_timeout_ticks() -> u64 {
    500
}

fn default_repl_flow_window_segments() -> u64 {
    64
}

/// Default committed-WAL growth that triggers a snapshot: 1 GiB.
pub(crate) fn default_snapshot_wal_threshold_bytes() -> u64 {
    1024 * 1024 * 1024
}

pub(crate) fn default_gc_check_interval_secs() -> u64 {
    60
}

pub(crate) fn default_sync_timeout_ms() -> u64 {
    10000
}

#[cfg(test)]
#[path = "../mudud_cfg_test.rs"]
mod mudud_cfg_test;
