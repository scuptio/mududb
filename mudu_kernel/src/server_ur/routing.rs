use crate::server_ur::fsm::ConnectionState;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    ConnectionId,
    PlayerId,
    RemoteHash,
}

#[derive(Debug, Clone)]
pub struct RoutingContext {
    conn_id: u64,
    remote_addr: SocketAddr,
    opt_player_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConnectionTransfer {
    conn_id: u64,
    target_worker: usize,
    state: ConnectionState,
    remote_addr: SocketAddr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOpenConfig {
    session_id: OID,
    partition_id: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOpenTransferAction {
    request_id: u64,
    config: SessionOpenConfig,
}

#[derive(Debug, Deserialize)]
struct RawSessionOpenConfig {
    session_id: OID,
    partition_id: usize,
}

impl RoutingContext {
    pub fn new(conn_id: u64, remote_addr: SocketAddr, opt_player_id: Option<String>) -> Self {
        Self {
            conn_id,
            remote_addr,
            opt_player_id,
        }
    }

    pub fn conn_id(&self) -> u64 {
        self.conn_id
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub fn opt_player_id(&self) -> Option<&str> {
        self.opt_player_id.as_deref()
    }
}

impl ConnectionTransfer {
    pub fn new(
        conn_id: u64,
        target_worker: usize,
        state: ConnectionState,
        remote_addr: SocketAddr,
    ) -> Self {
        Self {
            conn_id,
            target_worker,
            state,
            remote_addr,
        }
    }

    pub fn conn_id(&self) -> u64 {
        self.conn_id
    }

    pub fn target_worker(&self) -> usize {
        self.target_worker
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }
}

impl SessionOpenConfig {
    pub fn new(session_id: OID, partition_id: usize) -> Self {
        Self {
            session_id,
            partition_id,
        }
    }

    pub fn session_id(&self) -> OID {
        self.session_id
    }

    pub fn partition_id(&self) -> usize {
        self.partition_id
    }
}

impl SessionOpenTransferAction {
    pub fn new(request_id: u64, config: SessionOpenConfig) -> Self {
        Self { request_id, config }
    }

    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    pub fn config(&self) -> SessionOpenConfig {
        self.config
    }
}

pub fn route_worker(ctx: &RoutingContext, mode: RoutingMode, worker_count: usize) -> usize {
    let key = match mode {
        RoutingMode::ConnectionId => ctx.conn_id().to_string(),
        RoutingMode::PlayerId => ctx
            .opt_player_id()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| ctx.conn_id().to_string()),
        RoutingMode::RemoteHash => ctx.remote_addr().to_string(),
    };
    stable_hash(&key) % worker_count.max(1)
}

pub fn parse_session_open_config(
    config_json: Option<&str>,
    default_partition_id: usize,
    worker_count: usize,
) -> RS<SessionOpenConfig> {
    let config = match config_json {
        Some(raw) => {
            let parsed: RawSessionOpenConfig = serde_json::from_str(raw)
                .map_err(|e| m_error!(EC::ParseErr, "parse session open config json error", e))?;
            SessionOpenConfig::new(parsed.session_id, parsed.partition_id)
        }
        None => SessionOpenConfig::new(0, default_partition_id),
    };
    if config.partition_id() >= worker_count.max(1) {
        return Err(m_error!(
            EC::ParseErr,
            format!(
                "partition_id {} is out of range for {} workers",
                config.partition_id(),
                worker_count
            )
        ));
    }
    Ok(config)
}

fn stable_hash(value: &str) -> usize {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish() as usize
}
