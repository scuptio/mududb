use crate::backend::cfg_meta::ConfigMutability;
use crate::service::runtime_opt::ComponentTarget;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::env::home_dir;
use std::fmt::Display;
use std::path::{Path, PathBuf};

/// Backend server execution mode.
#[derive(Serialize_repr, Deserialize_repr, Eq, PartialEq, Debug, Clone, Copy, Default)]
#[repr(u8)]
pub enum ServerMode {
    /// Legacy mode.
    #[default]
    Legacy = 0,
    /// io_uring-based mode.
    IOUring = 1,
    /// Tokio-based mode.
    Tokio = 2,
}

/// TCP connection routing strategy.
#[derive(Serialize_repr, Deserialize_repr, Eq, PartialEq, Debug, Clone, Copy, Default)]
#[repr(u8)]
pub enum RoutingMode {
    /// Route by connection identifier.
    #[default]
    ConnectionId = 0,
    /// Route by player identifier.
    PlayerId = 1,
    /// Route by remote hash.
    RemoteHash = 2,
}

/// MuduDB server configuration.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct MuduDBCfg {
    /// Path to the application package.
    pub mpk_path: String,
    /// Path to the database directory.
    #[serde(alias = "data_path")]
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
    #[serde(default, alias = "io_uring_worker_threads")]
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
    /// io_uring log chunk size in bytes.
    #[serde(default = "default_io_uring_log_chunk_size")]
    pub io_uring_log_chunk_size: u64,
    /// Database page size in bytes.
    ///
    /// This is a `ConfigMutability::Persistent` setting: it is written into the
    /// on-disk format of page files. Changing it for an existing database
    /// requires a migration tool that rewrites all data files.
    #[serde(default = "default_page_size")]
    pub page_size: usize,
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
        writeln!(
            f,
            "  -> io_uring log chunk size: {}",
            self.io_uring_log_chunk_size
        )?;
        writeln!(f, "  -> page size: {}", self.page_size)?;
        writeln!(f, "-------------------")?;
        Ok(())
    }
}

impl Default for MuduDBCfg {
    fn default() -> Self {
        Self {
            mpk_path: mudu_sys::env_var::temp_dir().to_string_lossy().to_string(),
            db_path: mudu_sys::env_var::temp_dir().to_string_lossy().to_string(),
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: 8300,
            http_worker_threads: default_http_worker_threads(),
            pg_listen_port: 5432,
            component_target: None,
            enable_async: true,
            server_mode: ServerMode::Legacy,
            tcp_listen_port: default_tcp_listen_port(),
            tcp_multi_port: false,
            worker_threads: 0,
            io_uring_ring_entries: default_ring_entries(),
            io_uring_accept_multishot: true,
            io_uring_recv_multishot: true,
            io_uring_enable_fixed_buffers: false,
            io_uring_enable_fixed_files: false,
            routing_mode: RoutingMode::ConnectionId,
            io_uring_log_chunk_size: default_io_uring_log_chunk_size(),
            page_size: default_page_size(),
        }
    }
}

const MUDUDB_CFG_TOML_PATH: &str = ".mududb/mududb_cfg.toml";

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

fn default_io_uring_log_chunk_size() -> u64 {
    64 * 1024 * 1024
}

fn default_page_size() -> usize {
    4096
}

/// Load a MuduDB configuration from the given path or the default location.
pub fn load_mududb_cfg(opt_cfg_path: Option<String>) -> RS<MuduDBCfg> {
    let cfg_path = match opt_cfg_path {
        Some(cfg_path) => PathBuf::from(cfg_path),
        None => {
            let opt_home = home_dir();
            let home_path = match opt_home {
                Some(p) => p,
                None => return Err(mudu_error!(ErrorCode::NotFound, "no home path env setting")),
            };
            home_path.join(MUDUDB_CFG_TOML_PATH)
        }
    };

    if cfg_path.exists() {
        let cfg = read_mududb_cfg(cfg_path)?;
        Ok(cfg)
    } else {
        let cfg = MuduDBCfg::default();
        write_mududb_cfg(cfg_path, &cfg)?;
        Ok(cfg)
    }
}

fn read_mududb_cfg<P: AsRef<Path>>(path: P) -> RS<MuduDBCfg> {
    let s = mudu_sys::fs::sync::read_to_string(path.as_ref())?;
    let r = toml::from_str::<MuduDBCfg>(s.as_str());
    let cfg = r.map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            "deserialization MuduDB configuration file error",
            e
        )
    })?;
    Ok(cfg)
}

fn write_mududb_cfg<P: AsRef<Path>>(path: P, cfg: &MuduDBCfg) -> RS<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        mudu_sys::fs::sync::create_dir_all(parent)?;
    }
    let r = toml::to_string(cfg);
    let s = r.map_err(|e| mudu_error!(ErrorCode::Encode, "serialize configuration error", e))?;

    mudu_sys::fs::sync::write(path, s.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod _test {
    use crate::backend::cfg_meta::ConfigMutability;
    use crate::backend::mududb_cfg::{MuduDBCfg, read_mududb_cfg, write_mududb_cfg};

    #[test]
    fn test_conf() {
        let cfg = MuduDBCfg::default();
        let path = mudu_sys::env_var::temp_dir().join("mudu/mududb_cfg.toml");
        let r = write_mududb_cfg(path.clone(), &cfg);
        assert!(r.is_ok());
        let r = read_mududb_cfg(path.clone());
        assert!(r.is_ok());
        let conf1 = r.unwrap();
        assert_eq!(conf1, cfg);
    }

    #[test]
    fn test_conf_with_comments_and_numeric_enums() {
        let path = mudu_sys::env_var::temp_dir().join("mudu/mududb_cfg_with_comments.toml");
        if let Some(parent) = path.parent() {
            mudu_sys::fs::sync::create_dir_all(parent).unwrap();
        }
        mudu_sys::fs::sync::write(
            &path,
            r#"
# Example config with comments
mpk_path = "/tmp/mpk"
data_path = "/tmp/data"
listen_ip = "127.0.0.1"
http_listen_port = 8300
http_worker_threads = 1
pg_listen_port = 5432
enable_async = true

# 0 = Legacy
# 1 = IOUring
# 2 = Tokio
server_mode = 1
tcp_listen_port = 9527
worker_threads = 0
io_uring_ring_entries = 1024
io_uring_accept_multishot = true
io_uring_recv_multishot = true
io_uring_enable_fixed_buffers = false
io_uring_enable_fixed_files = false

# 0 = ConnectionId
# 1 = PlayerId
# 2 = RemoteHash
routing_mode = 0
"#,
        )
        .unwrap();

        let cfg = read_mududb_cfg(path).unwrap();
        assert_eq!(
            cfg.server_mode,
            crate::backend::mududb_cfg::ServerMode::IOUring
        );
        assert_eq!(
            cfg.routing_mode,
            crate::backend::mududb_cfg::RoutingMode::ConnectionId
        );
        assert_eq!(cfg.db_path, "/tmp/data");
        assert_eq!(cfg.http_worker_threads, 1);
    }

    #[test]
    fn test_page_size_default() {
        let cfg = MuduDBCfg::default();
        assert_eq!(cfg.page_size, 4096);
    }

    #[test]
    fn test_mutability_of_known_fields() {
        assert_eq!(
            MuduDBCfg::mutability_of("page_size"),
            ConfigMutability::Persistent
        );
        assert_eq!(
            MuduDBCfg::mutability_of("db_path"),
            ConfigMutability::Persistent
        );
        assert_eq!(
            MuduDBCfg::mutability_of("worker_threads"),
            ConfigMutability::RestartRequired
        );
        assert_eq!(
            MuduDBCfg::mutability_of("tcp_multi_port"),
            ConfigMutability::Runtime
        );
    }
}

#[cfg(test)]
#[path = "mududb_cfg_test.rs"]
mod mududb_cfg_test;
