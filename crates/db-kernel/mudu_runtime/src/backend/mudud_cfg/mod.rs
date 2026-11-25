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

mod cluster;
mod io;
mod replication;
#[cfg(test)]
mod tests;

pub use cluster::{
    ControlMemberCfg, ControlPlaneCfg, ControlReplicaCfg, DataGroupCfg, DataMemberCfg,
    DataPlaneCfg, DataReplicaCfg, PlacementCfg,
};
#[cfg(test)]
pub(crate) use io::load_mudud_cfg_with_local;
pub use io::{
    NODE_ID_FILE_NAME, default_cfg_path, init_mudud_cfg, load_mudud_cfg, load_or_create_node_id,
};
pub use replication::{BackupCfg, RaftReplCfg, RoutingMode, StreamCfg};

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
    /// Trusted package signer fingerprints (ed25519 public-key SHA-256, hex).
    /// When non-empty, package install/load requires a valid signature from a
    /// trusted fingerprint.
    #[serde(default)]
    pub trusted_key_fingerprints: Vec<String>,
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
    /// Maximum live messages per queue.
    #[serde(default = "default_queue_max_msgs")]
    pub queue_max_msgs: u64,
    /// Maximum encoded payload bytes per queue.
    #[serde(default = "default_queue_max_bytes")]
    pub queue_max_bytes: u64,
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
    /// Configuration-format version; must be 1.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Cluster-global unique name of this node. Present only in cluster mode.
    #[serde(default)]
    pub name: Option<String>,
    /// Cluster name, identical across all nodes of the cluster.
    #[serde(default)]
    pub cluster: Option<String>,
    /// Local inter-node channel listener port.
    #[serde(default)]
    pub channel_listen_port: Option<u16>,
    /// Fencing token of the topology; the operator bumps it on every
    /// topology change and all nodes must agree.
    #[serde(default)]
    pub topology_version: Option<u64>,
    /// Dedicated control plane. Omitted selects the Combined layout (the
    /// control replica set is the single data replica set).
    #[serde(default)]
    pub control_plane: Option<ControlPlaneCfg>,
    /// Data plane: the data replica sets, placements and rebalance scheduler.
    /// Absent means the server runs as a single self-contained node.
    #[serde(default)]
    pub data_plane: Option<DataPlaneCfg>,
    /// Backup and point-in-time recovery. Absent means backup-free.
    #[serde(default)]
    pub backup: Option<BackupCfg>,
    /// Enables the sparse epoch protocol (P4): the control plane advances the
    /// stable/current epoch in memory within a committed reserved range
    /// instead of committing an `Advance` record per tick. Off by default
    /// while the protocol rolls out; the committed per-tick path stays the
    /// default.
    #[serde(default)]
    pub epoch_sparse: bool,
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
        match &self.data_plane {
            Some(data_plane) => writeln!(
                f,
                "  -> Topology: {} data group(s), {} control plane",
                data_plane.group.len(),
                if self.control_plane.is_some() {
                    "dedicated"
                } else {
                    "combined"
                }
            )?,
            None => writeln!(f, "  -> Topology: single node")?,
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
            trusted_key_fingerprints: Vec::new(),
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
            queue_max_msgs: default_queue_max_msgs(),
            queue_max_bytes: default_queue_max_bytes(),
            page_size: default_page_size(),
            wal_flush_max_wait_us: default_wal_flush_max_wait_us(),
            wal_sync_mode: default_wal_sync_mode(),
            wal_sync_interval_ms: default_wal_sync_interval_ms(),
            version: default_version(),
            name: None,
            cluster: None,
            channel_listen_port: None,
            topology_version: None,
            control_plane: None,
            data_plane: None,
            backup: None,
            epoch_sparse: false,
        }
    }
}

const MUDUDB_CFG_FILE_NAME: &str = "mudud.cfg";
const MUDUDB_CONFIG_DIR: &str = ".mududb";

const DEFAULT_CFG_TEMPLATE: &str = r#"# MuduDB server configuration
# Generated by mudud. Edit as needed.

# Configuration-format version. Currently 1.
version = 1

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
queue_max_msgs = 65536
queue_max_bytes = 67108864

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

# Cluster topology: top-level identity plus the control/data planes. Uncomment
# to enable; absent means a single self-contained node. The topology must be
# identical on every node except name; bump topology_version on every
# topology change.
# 集群拓扑配置。取消注释以启用；缺省时为单机模式。除 name 外整段须在
# 所有节点上保持一致；每次拓扑变更须递增 topology_version。
# name = "g1-node-1"
# cluster = "prod"
# channel_listen_port = 29527   # inter-node channel listener
# topology_version = 1
#
# Omit [control_plane] for the Combined layout (exactly one data group).
# Provide [control_plane] for a dedicated control plane (Raft only).
# 省略 [control_plane] 即 Combined（恰好一个数据组）；提供 [control_plane]
# 即独立控制面（仅 Raft）。
# [control_plane.replica.raft]
#
# [[control_plane.replica.member]]
# name = "ctl-1"
# host = "127.0.0.1"
# channel_port = 29527
# client_port = 5432
#
# [data_plane]
# rebalance_interval_ms = 0
#
# [[data_plane.group]]
# name = "grp-1"
# worker_count = 2
#
# [data_plane.group.replica.raft]
# tick_ms = 50                    # optional: raft state machine tick interval
# election_timeout_ticks = 500    # optional: election timeout in ticks
#
# [[data_plane.group.replica.member]]
# name = "g1-node-1"
# host = "127.0.0.1"
# channel_port = 29527
# client_port = 5432
# replication_port = 19527
#
# [[data_plane.group.replica.member]]
# name = "g1-node-2"
# host = "127.0.0.1"
# channel_port = 29528
# client_port = 5433
# replication_port = 19528
#
# Stream mode (replaces the raft subsection above):
# 流复制模式（替换上面的 raft 子段）：
# [data_plane.group.replica.stream]
# bootstrap_primary = "g1-node-1"
# sync = true
# sync_timeout_ms = 10000
#
# [[data_plane.placements]]
# rule = "r"
# partition = "p1"
# group = "grp-1"
# worker_index = 0
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

    /// Validates the topology sections: `version` must be 1; in cluster mode
    /// the top-level identity fields must be present together; each data group
    /// must declare at most one replication mode; names and member addresses
    /// must be unique; and the Combined/dedicated membership rules must hold.
    pub fn validate_topology(&self) -> RS<()> {
        if self.version != 1 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("unsupported configuration version {}", self.version)
            ));
        }
        let Some(data_plane) = &self.data_plane else {
            if self.control_plane.is_some() {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    "control_plane requires data_plane"
                ));
            }
            return Ok(());
        };
        self.validate_cluster_identity()?;
        if data_plane.group.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "data_plane.group must not be empty"
            ));
        }
        if self.control_plane.is_none() && data_plane.group.len() != 1 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "Combined layout (no control_plane) requires exactly one data group"
            ));
        }
        let node_names = self.validate_data_groups(data_plane)?;
        self.validate_control_plane(&node_names)?;
        self.validate_member_addresses(data_plane)?;
        self.validate_local_membership(data_plane)?;
        self.validate_channel_port()?;
        Ok(())
    }

    /// Cluster mode requires the top-level identity fields to be present.
    fn validate_cluster_identity(&self) -> RS<()> {
        if self.name.is_none()
            || self.cluster.is_none()
            || self.channel_listen_port.is_none()
            || self.topology_version.is_none()
        {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "cluster mode requires name, cluster, channel_listen_port and topology_version"
            ));
        }
        Ok(())
    }

    /// Validates each data group and returns the set of data-member names.
    fn validate_data_groups(
        &self,
        data_plane: &DataPlaneCfg,
    ) -> RS<std::collections::HashSet<String>> {
        let mut group_names = std::collections::HashSet::new();
        let mut node_names = std::collections::HashSet::new();
        for group in &data_plane.group {
            if !group_names.insert(group.name.as_str()) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("duplicate data group name {}", group.name)
                ));
            }
            if group.worker_count == 0 {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("data group {} has zero workers", group.name)
                ));
            }
            if group.replica.member.is_empty() {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("data group {} has no members", group.name)
                ));
            }
            self.validate_group_mode(group)?;
            if let Some(stream) = &group.replica.stream {
                if !group
                    .replica
                    .member
                    .iter()
                    .any(|member| member.name == stream.bootstrap_primary)
                {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!(
                            "data group {} bootstrap_primary {} is not a member",
                            group.name, stream.bootstrap_primary
                        )
                    ));
                }
                if stream.sync && group.replica.member.len() < 2 {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!(
                            "data group {} stream sync=true requires at least two members",
                            group.name
                        )
                    ));
                }
            }
            for member in &group.replica.member {
                if !node_names.insert(member.name.clone()) {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!("node name {} appears in more than one group", member.name)
                    ));
                }
            }
        }
        Ok(node_names)
    }

    /// A data group must declare at most one replication mode. With neither
    /// mode, the group is a single replica and must have exactly one member.
    fn validate_group_mode(&self, group: &DataGroupCfg) -> RS<()> {
        match (group.replica.raft.is_some(), group.replica.stream.is_some()) {
            (true, true) => Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("data group {} declares both raft and stream", group.name)
            )),
            (false, false) => {
                if group.replica.member.len() != 1 {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!(
                            "data group {} with neither raft nor stream must have exactly one member",
                            group.name
                        )
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The dedicated control plane must be non-empty, Raft-declared, and
    /// disjoint from the data members.
    fn validate_control_plane(&self, node_names: &std::collections::HashSet<String>) -> RS<()> {
        let Some(control) = &self.control_plane else {
            return Ok(());
        };
        if control.replica.member.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "control_plane.replica.member must not be empty"
            ));
        }
        if control.replica.raft.is_none() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "control_plane.replica.raft must be declared"
            ));
        }
        let mut control_names = std::collections::HashSet::new();
        for member in &control.replica.member {
            if !control_names.insert(member.name.clone()) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("duplicate control node {}", member.name)
                ));
            }
            if node_names.contains(&member.name) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("control node {} also belongs to a data group", member.name)
                ));
            }
        }
        Ok(())
    }

    /// Member `(host, port)` pairs must be unique within their role, and the
    /// local member's ports must not collide with the client-facing ports.
    fn validate_member_addresses(&self, data_plane: &DataPlaneCfg) -> RS<()> {
        let mut repl_addrs = std::collections::HashSet::new();
        let mut channel_addrs = std::collections::HashSet::new();
        let mut client_addrs = std::collections::HashSet::new();
        for member in data_plane
            .group
            .iter()
            .flat_map(|group| group.replica.member.iter())
        {
            if !repl_addrs.insert((member.host.as_str(), member.replication_port)) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "duplicate replication address {}:{}",
                        member.host, member.replication_port
                    )
                ));
            }
            if !channel_addrs.insert((member.host.as_str(), member.channel_port)) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "duplicate channel address {}:{}",
                        member.host, member.channel_port
                    )
                ));
            }
            if !client_addrs.insert((member.host.as_str(), member.client_port)) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "duplicate client address {}:{}",
                        member.host, member.client_port
                    )
                ));
            }
        }
        if let Some(control) = &self.control_plane {
            for member in &control.replica.member {
                if !channel_addrs.insert((member.host.as_str(), member.channel_port)) {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!(
                            "duplicate channel address {}:{}",
                            member.host, member.channel_port
                        )
                    ));
                }
                if !client_addrs.insert((member.host.as_str(), member.client_port)) {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!(
                            "duplicate client address {}:{}",
                            member.host, member.client_port
                        )
                    ));
                }
            }
        }
        if let Some(local) = self.local_data_group().and_then(|group| {
            group
                .replica
                .member
                .iter()
                .find(|member| Some(&member.name) == self.name.as_ref())
        }) {
            for (name, port) in [
                ("http_listen_port", self.http_listen_port),
                ("pg_listen_port", self.pg_listen_port),
                ("tcp_listen_port", self.tcp_listen_port),
            ] {
                if local.replication_port == port || local.channel_port == port {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!("member {} port {} collides with {}", local.name, port, name)
                    ));
                }
            }
        }
        Ok(())
    }

    /// The local node's name must occur in a data group or the control plane.
    fn validate_local_membership(&self, data_plane: &DataPlaneCfg) -> RS<()> {
        let name = self.name.as_deref().unwrap_or("");
        let in_group = data_plane
            .group
            .iter()
            .any(|group| group.replica.member.iter().any(|m| m.name == name));
        let in_control = self.control_plane.as_ref().is_some_and(|control| {
            control
                .replica
                .member
                .iter()
                .any(|member| member.name == name)
        });
        if !in_group && !in_control {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "node name {} does not belong to any data group or the control plane",
                    name
                )
            ));
        }
        Ok(())
    }

    /// The channel listener port must not collide with the client-facing ports.
    fn validate_channel_port(&self) -> RS<()> {
        let channel_port = self.channel_listen_port.unwrap_or(0);
        for (name, port) in [
            ("http_listen_port", self.http_listen_port),
            ("pg_listen_port", self.pg_listen_port),
            ("tcp_listen_port", self.tcp_listen_port),
        ] {
            if channel_port == port {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "channel_listen_port {} collides with {}",
                        channel_port, name
                    )
                ));
            }
        }
        Ok(())
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

    /// The data group containing this node, when this node is a data member.
    pub fn local_data_group(&self) -> Option<&DataGroupCfg> {
        let name = self.name.as_ref()?;
        self.data_plane.as_ref()?.group.iter().find(|group| {
            group
                .replica
                .member
                .iter()
                .any(|member| &member.name == name)
        })
    }

    /// Whether this node is a dedicated control-plane node: a control member
    /// of no data group (it runs no business workers).
    pub fn is_control_node(&self) -> bool {
        let name = match &self.name {
            Some(name) => name,
            None => return false,
        };
        if self.local_data_group().is_some() {
            return false;
        }
        self.control_plane.as_ref().is_some_and(|control| {
            control
                .replica
                .member
                .iter()
                .any(|member| &member.name == name)
        })
    }
}

fn default_true() -> bool {
    true
}

fn default_version() -> u32 {
    1
}

/// FNV-1a 64-bit hash over the UTF-8 bytes of `s` (offset basis
/// `0xcbf29ce484222325`, prime `0x100000001b3`, no salt/seed). This is the
/// contract-pinned derivation of the internal `node_id` / `group_id` /
/// `cluster_id` from their names.
pub(crate) fn fnv1a64(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in s.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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

pub(crate) fn default_repl_flow_window_segments() -> u64 {
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

fn default_queue_max_msgs() -> u64 {
    65536
}
fn default_queue_max_bytes() -> u64 {
    64 * 1024 * 1024
}
