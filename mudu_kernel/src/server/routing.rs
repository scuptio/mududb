use crate::server::worker_registry::WorkerRegistry;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::de::{self, Deserializer};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    ConnectionId,
    PlayerId,
    RemoteHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOpenConfig {
    session_id: OID,
    worker_id: OID,
    target_worker_index: usize,
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

pub fn parse_session_open_config(
    config_json: Option<&str>,
    default_worker_index: usize,
    default_worker_id: OID,
    registry: &WorkerRegistry,
) -> RS<SessionOpenConfig> {
    match config_json {
        Some(raw) => {
            let parsed: RawSessionOpenConfig = serde_json::from_str(raw).map_err(|e| {
                mudu_error!(ErrorCode::Parse, "parse session open config json error", e)
            })?;
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
                        mudu_error!(
                            ErrorCode::EntityNotFound,
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
