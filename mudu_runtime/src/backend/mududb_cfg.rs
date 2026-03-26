use crate::service::runtime_opt::{ComponentTarget, RuntimeTarget};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::env::{home_dir, temp_dir};
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize_repr, Deserialize_repr, Eq, PartialEq, Debug, Clone, Copy, Default)]
#[repr(u8)]
pub enum ServerMode {
    #[default]
    Legacy = 0,
    IOUring = 1,
}

#[derive(Serialize_repr, Deserialize_repr, Eq, PartialEq, Debug, Clone, Copy, Default)]
#[repr(u8)]
pub enum RoutingMode {
    #[default]
    ConnectionId = 0,
    PlayerId = 1,
    RemoteHash = 2,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct MuduDBCfg {
    pub mpk_path: String,
    pub data_path: String,
    pub listen_ip: String,
    pub http_listen_port: u16,
    #[serde(default = "default_http_worker_threads")]
    pub http_worker_threads: usize,
    pub pg_listen_port: u16,
    #[serde(default)]
    pub component_target: Option<ComponentTarget>,
    #[serde(default)]
    pub enable_p2: bool,
    pub enable_async: bool,
    #[serde(default)]
    pub server_mode: ServerMode,
    #[serde(default = "default_tcp_listen_port")]
    pub tcp_listen_port: u16,
    #[serde(default)]
    pub io_uring_worker_threads: usize,
    #[serde(default = "default_ring_entries")]
    pub io_uring_ring_entries: u32,
    #[serde(default = "default_true")]
    pub io_uring_accept_multishot: bool,
    #[serde(default = "default_true")]
    pub io_uring_recv_multishot: bool,
    #[serde(default)]
    pub io_uring_enable_fixed_buffers: bool,
    #[serde(default)]
    pub io_uring_enable_fixed_files: bool,
    #[serde(default)]
    pub routing_mode: RoutingMode,
    #[serde(default = "default_io_uring_log_chunk_size")]
    pub io_uring_log_chunk_size: u64,
}

impl Display for MuduDBCfg {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let runtime_target = self.runtime_target();
        write!(f, "MuduDB Setting:\n")?;
        write!(f, "-------------------\n")?;
        write!(f, "  -> Package path: {}\n", self.mpk_path)?;
        write!(f, "  -> Data path: {}\n", self.data_path)?;
        write!(f, "  -> Listen IP address: {}\n", self.listen_ip)?;
        write!(f, "  -> HTTP Listening port: {}\n", self.http_listen_port)?;
        write!(
            f,
            "  -> HTTP worker threads: {}\n",
            self.http_worker_threads
        )?;
        write!(f, "  -> PG Listening port: {}\n", self.pg_listen_port)?;
        write!(f, "  -> Runtime target: {:?}\n", runtime_target)?;
        write!(f, "  -> Enable Async: {}\n", self.enable_async)?;
        write!(f, "  -> Server mode: {:?}\n", self.server_mode)?;
        write!(f, "  -> TCP Listening port: {}\n", self.tcp_listen_port)?;
        write!(
            f,
            "  -> io_uring workers: {}\n",
            self.io_uring_worker_threads
        )?;
        write!(
            f,
            "  -> io_uring ring entries: {}\n",
            self.io_uring_ring_entries
        )?;
        write!(
            f,
            "  -> io_uring accept multishot: {}\n",
            self.io_uring_accept_multishot
        )?;
        write!(
            f,
            "  -> io_uring recv multishot: {}\n",
            self.io_uring_recv_multishot
        )?;
        write!(
            f,
            "  -> io_uring fixed buffers: {}\n",
            self.io_uring_enable_fixed_buffers
        )?;
        write!(
            f,
            "  -> io_uring fixed files: {}\n",
            self.io_uring_enable_fixed_files
        )?;
        write!(f, "  -> Routing mode: {:?}\n", self.routing_mode)?;
        write!(
            f,
            "  -> io_uring log chunk size: {}\n",
            self.io_uring_log_chunk_size
        )?;
        write!(f, "-------------------\n")?;
        Ok(())
    }
}

impl Default for MuduDBCfg {
    fn default() -> Self {
        Self {
            mpk_path: temp_dir().to_str().unwrap().to_string(),
            data_path: temp_dir().to_str().unwrap().to_string(),
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: 8300,
            http_worker_threads: default_http_worker_threads(),
            pg_listen_port: 5432,
            component_target: None,
            enable_p2: false,
            enable_async: false,
            server_mode: ServerMode::Legacy,
            tcp_listen_port: default_tcp_listen_port(),
            io_uring_worker_threads: 0,
            io_uring_ring_entries: default_ring_entries(),
            io_uring_accept_multishot: true,
            io_uring_recv_multishot: true,
            io_uring_enable_fixed_buffers: false,
            io_uring_enable_fixed_files: false,
            routing_mode: RoutingMode::ConnectionId,
            io_uring_log_chunk_size: default_io_uring_log_chunk_size(),
        }
    }
}

const MUDUDB_CFG_TOML_PATH: &str = ".mudu/mududb_cfg.toml";

impl MuduDBCfg {
    pub fn runtime_target(&self) -> RuntimeTarget {
        match self.component_target {
            Some(target) => RuntimeTarget::Component(target),
            None if self.enable_p2 => RuntimeTarget::Component(ComponentTarget::P2),
            None => RuntimeTarget::P1,
        }
    }

    pub fn effective_worker_threads(&self) -> usize {
        if self.io_uring_worker_threads > 0 {
            self.io_uring_worker_threads
        } else {
            std::thread::available_parallelism()
                .map(|v| v.get())
                .unwrap_or(1)
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

pub fn load_mududb_cfg(opt_cfg_path: Option<String>) -> RS<MuduDBCfg> {
    let cfg_path = match opt_cfg_path {
        Some(cfg_path) => PathBuf::from(cfg_path),
        None => {
            let opt_home = home_dir();
            let home_path = match opt_home {
                Some(p) => p,
                None => return Err(m_error!(EC::IOErr, "no home path env setting")),
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
    let r = fs::read_to_string(path);
    let s = r.map_err(|e| m_error!(EC::IOErr, "read MuduDB configuration error", e))?;
    let r = toml::from_str::<MuduDBCfg>(s.as_str());
    let cfg = r.map_err(|e| {
        m_error!(
            EC::IOErr,
            "deserialization MuduDB configuration file error",
            e
        )
    })?;
    Ok(cfg)
}

fn write_mududb_cfg<P: AsRef<Path>>(path: P, cfg: &MuduDBCfg) -> RS<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| m_error!(EC::IOErr, "create directory error", e))?;
        }
    }
    let r = toml::to_string(cfg);
    let s = r.map_err(|e| m_error!(EC::EncodeErr, "serialize configuration error", e))?;

    let r = fs::write(path, s);
    r.map_err(|e| m_error!(EC::IOErr, "write configuration file error", e))?;
    Ok(())
}

#[cfg(test)]
mod _test {
    use crate::backend::mududb_cfg::{MuduDBCfg, read_mududb_cfg, write_mududb_cfg};
    use std::env::temp_dir;
    use std::fs;
    #[test]
    fn test_conf() {
        let cfg = MuduDBCfg::default();
        let path = temp_dir().join("mudu/mududb_cfg.toml");
        let r = write_mududb_cfg(path.clone(), &cfg);
        assert!(r.is_ok());
        let r = read_mududb_cfg(path.clone());
        assert!(r.is_ok());
        let conf1 = r.unwrap();
        assert_eq!(conf1, cfg);
    }

    #[test]
    fn test_conf_with_comments_and_numeric_enums() {
        let path = temp_dir().join("mudu/mududb_cfg_with_comments.toml");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(
            &path,
            r#"
# Example config with comments
mpk_path = "/tmp/mpk"
data_path = "/tmp/data"
listen_ip = "127.0.0.1"
http_listen_port = 8300
http_worker_threads = 1
pg_listen_port = 5432
enable_p2 = true
enable_async = true

# 0 = Legacy
# 1 = IOUring
server_mode = 1
tcp_listen_port = 9527
io_uring_worker_threads = 0
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
        assert_eq!(cfg.http_worker_threads, 1);
    }
}
