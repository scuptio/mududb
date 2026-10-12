use crate::worker_registry::WorkerRegistry;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::de::{self, Deserializer};
use serde::Deserialize;

/// TCP connection routing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    /// Route by connection identifier.
    ConnectionId,
    /// Route by player identifier.
    PlayerId,
    /// Route by remote hash.
    RemoteHash,
}

/// Validated routing decision for a session-open request: the session to
/// bind and the worker that must serve it.
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
    /// Create a routing decision binding `session_id` to the worker with the
    /// given id and index.
    pub fn new(session_id: OID, worker_id: OID, target_worker_index: usize) -> Self {
        Self {
            session_id,
            worker_id,
            target_worker_index,
        }
    }

    pub(crate) fn session_id(&self) -> OID {
        self.session_id
    }

    /// The id of the worker that must serve the session.
    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    /// The index of the worker that must serve the session.
    pub fn target_worker_index(&self) -> usize {
        self.target_worker_index
    }
}

/// Parse the optional JSON routing config of a session-open request into a
/// [`SessionOpenConfig`].
///
/// A missing document, or a `worker_id` of 0, targets the accepting worker
/// (`default_worker_index` / `default_worker_id`); any other `worker_id` is
/// resolved to a worker index through `registry` and fails with
/// `EntityNotFound` when no such worker exists. OID fields accept a number,
/// a decimal string, or a `{h, l}` pair.
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
