use crate::server_ur::fsm::ConnectionState;
use crate::server_ur::worker_registry::WorkerRegistry;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use serde::de::{self, Deserializer};
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
    worker_id: OID,
    target_worker_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOpenTransferAction {
    request_id: u64,
    config: SessionOpenConfig,
}

#[derive(Debug, Deserialize)]
struct RawSessionOpenConfig {
    #[serde(deserialize_with = "deserialize_oid_json")]
    session_id: OID,
    #[serde(default, deserialize_with = "deserialize_opt_oid_json")]
    worker_id: Option<OID>,
}

#[derive(Debug, Deserialize)]
struct RawUniOid {
    h: u64,
    l: u64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawOidJson {
    Number(u64),
    String(String),
    UniOid(RawUniOid),
}

fn deserialize_oid_json<'de, D>(deserializer: D) -> Result<OID, D::Error>
where
    D: Deserializer<'de>,
{
    match RawOidJson::deserialize(deserializer)? {
        RawOidJson::Number(value) => Ok(value as OID),
        RawOidJson::String(value) => value.parse::<OID>().map_err(de::Error::custom),
        RawOidJson::UniOid(value) => Ok(((value.h as u128) << 64) | (value.l as u128)),
    }
}

fn deserialize_opt_oid_json<'de, D>(deserializer: D) -> Result<Option<OID>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<RawOidJson>::deserialize(deserializer)?
        .map(|value| match value {
            RawOidJson::Number(value) => Ok(value as OID),
            RawOidJson::String(value) => value.parse::<OID>().map_err(de::Error::custom),
            RawOidJson::UniOid(value) => Ok(((value.h as u128) << 64) | (value.l as u128)),
        })
        .transpose()
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
    pub fn new(session_id: OID, worker_id: OID, target_worker_index: usize) -> Self {
        Self {
            session_id,
            worker_id,
            target_worker_index,
        }
    }

    pub fn session_id(&self) -> OID {
        self.session_id
    }

    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    pub fn target_worker_index(&self) -> usize {
        self.target_worker_index
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
    default_worker_index: usize,
    default_worker_id: OID,
    registry: &WorkerRegistry,
) -> RS<SessionOpenConfig> {
    match config_json {
        Some(raw) => {
            let parsed: RawSessionOpenConfig = serde_json::from_str(raw)
                .map_err(|e| m_error!(EC::ParseErr, "parse session open config json error", e))?;
            let worker_id = parsed.worker_id.unwrap_or(default_worker_id);
            if worker_id == 0 {
                return Ok(SessionOpenConfig::new(
                    parsed.session_id,
                    default_worker_id,
                    default_worker_index,
                ));
            }
            let target_worker_index =
                registry
                    .worker_index_by_worker_id(worker_id)
                    .ok_or_else(|| {
                        m_error!(
                            EC::NoSuchElement,
                            format!("no such worker id {}", worker_id)
                        )
                    })?;
            Ok(SessionOpenConfig::new(
                parsed.session_id,
                worker_id,
                target_worker_index,
            ))
        }
        None => Ok(SessionOpenConfig::new(
            0,
            default_worker_id,
            default_worker_index,
        )),
    }
}

fn stable_hash(value: &str) -> usize {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish() as usize
}
