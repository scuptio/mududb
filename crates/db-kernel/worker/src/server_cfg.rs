use crate::routing::RoutingMode;
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::storage::page::page_block_ref::DEFAULT_PAGE_SIZE;
use ::wal::worker_log::WalSyncPolicy;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::net::sync::StdTcpListener;
use mudu_utils::oid::gen_oid;
use std::net::SocketAddr;
use std::time::Duration;

// Replication config moved down to `::catalog::repl_cfg` so the `repl`
// layer does not depend on the server config.
pub use ::catalog::repl_cfg::{ReplMode, ReplNodeCfg, ReplPeerCfg, ReplRole};

/// Default group-commit batching window: how long a queued commit batch may
/// age before the flush driver flushes it, when byte/frame watermarks are
/// not met first.
pub(crate) const DEFAULT_LOG_BATCHING_MAX_WAIT: Duration = Duration::from_micros(200);

/// WAL archiving mode. Controls whether worker WAL chunks are continuously
/// archived to `archive_dir` before prefix GC may reclaim them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchiveMode {
    /// No archiving (the default; WAL GC behaves as before).
    #[default]
    Off,
    /// Archive sealed chunks on the periodic archiver cadence.
    On,
    /// Archive and require synchronous durability of the archive copy before
    /// a commit acknowledges (not yet wired into the commit path in P2).
    Always,
}

/// Backup / PITR configuration. `None` keeps the server backup-free.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackupCfg {
    archive_dir: Option<String>,
    backup_dir: Option<String>,
    archive_mode: ArchiveMode,
}

impl BackupCfg {
    /// Builds a backup configuration.
    pub fn new(
        archive_dir: Option<String>,
        backup_dir: Option<String>,
        archive_mode: ArchiveMode,
    ) -> Self {
        Self {
            archive_dir,
            backup_dir,
            archive_mode,
        }
    }

    /// The WAL archive directory, when archiving is enabled.
    pub fn archive_dir(&self) -> Option<&str> {
        self.archive_dir.as_deref()
    }

    /// The base-backup output directory.
    pub fn backup_dir(&self) -> Option<&str> {
        self.backup_dir.as_deref()
    }

    /// The archiving mode.
    pub fn archive_mode(&self) -> ArchiveMode {
        self.archive_mode
    }

    /// True when continuous WAL archiving is enabled (an archive dir is set
    /// and the mode is not [`ArchiveMode::Off`]).
    pub fn archiving_enabled(&self) -> bool {
        self.archive_dir.is_some() && self.archive_mode != ArchiveMode::Off
    }
}

/// Configuration shared by both execution paths of the `client` backend.
///
/// The same configuration is consumed by both the io_uring worker-ring backend
/// and the Tokio backend so they keep the worker model and protocol surface
/// aligned.
#[derive(Debug)]
pub struct ServerCfg {
    server_instance_id: ServerInstanceId,
    worker_count: usize,
    listen_ip: String,
    listen_port: u16,
    multi_port: bool,
    data_dir: String,
    log_dir: String,
    log_chunk_size: u64,
    routing_mode: RoutingMode,
    page_size: usize,
    log_batching_max_wait: Duration,
    wal_sync_policy: WalSyncPolicy,
    replication: Option<ReplNodeCfg>,
    backup: Option<BackupCfg>,
}

impl ServerCfg {
    /// Creates a backend configuration.
    ///
    /// The resulting value can be used by both the io_uring and Tokio TCP
    /// backends with the same externally visible behavior.
    ///
    /// The page size defaults to [`DEFAULT_PAGE_SIZE`] and can be changed with
    /// [`Self::with_page_size`] before the database directory is created.
    pub fn new(
        worker_count: usize,
        listen_ip: String,
        listen_port: u16,
        data_dir: String,
        log_dir: String,
        routing_mode: RoutingMode,
    ) -> RS<Self> {
        Ok(Self {
            server_instance_id: gen_oid(),
            worker_count,
            listen_ip,
            listen_port,
            multi_port: false,
            data_dir,
            log_dir,
            log_chunk_size: 64 * 1024 * 1024,
            routing_mode,
            page_size: DEFAULT_PAGE_SIZE,
            log_batching_max_wait: DEFAULT_LOG_BATCHING_MAX_WAIT,
            wal_sync_policy: WalSyncPolicy::Commit,
            replication: None,
            backup: None,
        })
    }

    /// Overrides the WAL chunk size in bytes (default 64 MiB).
    pub fn with_log_chunk_size(mut self, log_chunk_size: u64) -> Self {
        self.log_chunk_size = log_chunk_size;
        self
    }

    /// Overrides the group-commit batching window (how long a queued commit
    /// batch may age before the flush driver flushes it). Larger values
    /// improve fsync sharing under concurrency; smaller values lower
    /// lone-commit latency.
    pub fn with_log_batching_max_wait(mut self, max_wait: Duration) -> Self {
        self.log_batching_max_wait = max_wait;
        self
    }

    /// Overrides the WAL durability policy. Defaults to
    /// [`WalSyncPolicy::Commit`] (fsync every flush round); see
    /// [`WalSyncPolicy::Periodic`] for the bounded-loss mode.
    pub fn with_wal_sync_policy(mut self, wal_sync_policy: WalSyncPolicy) -> Self {
        self.wal_sync_policy = wal_sync_policy;
        self
    }

    /// Enables one listen port per worker (`listen_port + worker_index`)
    /// instead of a single shared port.
    pub fn with_multi_port(mut self, multi_port: bool) -> Self {
        self.multi_port = multi_port;
        self
    }

    /// Attaches a primary-replica replication configuration. `None` keeps the
    /// server in single-node mode (no replication components are started).
    pub fn with_replication(mut self, replication: Option<ReplNodeCfg>) -> Self {
        self.replication = replication;
        self
    }

    /// Attaches a backup / PITR configuration. `None` keeps the server
    /// backup-free (WAL GC behaves as before).
    pub fn with_backup(mut self, backup: Option<BackupCfg>) -> Self {
        self.backup = backup;
        self
    }

    /// Overrides the database page size.
    ///
    /// `page_size` must be a power of two and at least `DEFAULT_PAGE_SIZE`.
    /// This value is a `Persistent` config: changing it for an existing data
    /// directory requires a migration tool.
    pub fn with_page_size(mut self, page_size: usize) -> RS<Self> {
        if page_size < DEFAULT_PAGE_SIZE {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "page_size {} is below minimum {}",
                    page_size, DEFAULT_PAGE_SIZE
                )
            ));
        }
        if !page_size.is_power_of_two() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("page_size {} is not a power of two", page_size)
            ));
        }
        self.page_size = page_size;
        Ok(self)
    }

    /// The generated id of this server instance.
    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

    /// The group-commit batching window used to build the WAL batching
    /// watermarks.
    pub fn log_batching_max_wait(&self) -> Duration {
        self.log_batching_max_wait
    }

    /// The WAL durability policy applied to every worker log.
    pub fn wal_sync_policy(&self) -> WalSyncPolicy {
        self.wal_sync_policy
    }

    /// The number of workers of the server.
    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// The TCP listen IP address.
    pub fn listen_ip(&self) -> &str {
        &self.listen_ip
    }

    /// The base TCP listen port.
    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    /// Whether each worker listens on its own port.
    pub fn multi_port(&self) -> bool {
        self.multi_port
    }

    /// The listen port of `worker_index`: the base port, or base port plus
    /// the worker index when multi-port mode is enabled.
    pub fn listen_port_for_worker(&self, worker_index: usize) -> RS<u16> {
        if !self.multi_port {
            return Ok(self.listen_port);
        }
        let worker_offset = u16::try_from(worker_index).map_err(|_| {
            mudu_error!(
                ErrorCode::Parse,
                format!("worker index too large for port mapping: {}", worker_index)
            )
        })?;
        self.listen_port.checked_add(worker_offset).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "worker listen port overflow: base_port={}, worker_index={}",
                    self.listen_port, worker_index
                )
            )
        })
    }

    /// Binds one TCP listener per worker on the calling thread.
    ///
    /// Both TCP backends accept pre-bound listeners via
    /// `ServerLaunch::with_prebound_listeners`. Binding every worker port up
    /// front (before any worker thread or management service starts) makes a
    /// listen port conflict fail startup immediately, instead of leaving one
    /// worker unbound while the rest of the server keeps answering
    /// `server is not ready`.
    pub fn prebind_worker_listeners(&self) -> RS<Vec<StdTcpListener>> {
        let mut listeners = Vec::with_capacity(self.worker_count);
        for worker_index in 0..self.worker_count {
            let worker_port = self.listen_port_for_worker(worker_index)?;
            let listen_addr: SocketAddr = format!("{}:{}", self.listen_ip, worker_port)
                .parse()
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Parse,
                        format!("parse tcp listen address error: {}", worker_port),
                        e
                    )
                })?;
            listeners.push(mudu_sys::net::sync::bind_tcp(listen_addr)?);
        }
        Ok(listeners)
    }

    /// The WAL (worker log) directory.
    pub fn log_dir(&self) -> &str {
        &self.log_dir
    }

    /// The data directory.
    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    /// The WAL chunk size in bytes.
    pub fn log_chunk_size(&self) -> u64 {
        self.log_chunk_size
    }

    /// The session routing mode.
    pub fn routing_mode(&self) -> RoutingMode {
        self.routing_mode
    }

    /// The database page size.
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The replication configuration of this server, when enabled.
    pub fn replication(&self) -> Option<&ReplNodeCfg> {
        self.replication.as_ref()
    }

    /// The backup / PITR configuration of this server, when enabled.
    pub fn backup(&self) -> Option<&BackupCfg> {
        self.backup.as_ref()
    }
}
