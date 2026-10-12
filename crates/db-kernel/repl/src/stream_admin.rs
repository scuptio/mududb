//! Stream-mode runtime role administration: the `stream_epoch` /
//! `stream_role` persistent state and the promote/follow orchestration.
//!
//! Epoch discipline mirrors the raft term: a promotion picks
//! `max(persisted, known) + 1` and persists it BEFORE the write gate
//! opens, so a zombie old primary's lower-epoch stream is rejected by
//! every replica that has seen a newer epoch (the data-plane backstop to
//! fencing). `stream_role` records `{role, epoch, primary_node_id,
//! primary_host, primary_port}` and wins over the static `primary` config
//! key at startup (stored wins, same philosophy as the raft stored conf).

use crate::ctl::ReplModeCtl;
use ::catalog::contract::message_bus_api::ServerInstanceId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{encode_repl_subscribe_request, MessageType, ReplSubscribeReq};
use mudu_sys::sync::SMutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// File name of the persisted stream epoch inside the database directory.
pub(crate) const STREAM_EPOCH_FILE_NAME: &str = "stream_epoch";
/// File name of the persisted stream role inside the database directory.
pub(crate) const STREAM_ROLE_FILE_NAME: &str = "stream_role";

/// Timeout of the identity probe a `follow` runs against the target
/// primary before accepting it.
const FOLLOW_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The node's role in a stream cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamRoleKind {
    /// Accepts writes and streams its WAL to replicas.
    Primary,
    /// Applies the primary's stream and serves reads only.
    Replica,
}

/// The persisted stream role of a node (`<db_path>/stream_role`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamRole {
    /// This node's role.
    pub role: StreamRoleKind,
    /// Highest known stream epoch.
    pub epoch: u64,
    /// Node id of the primary this node is or follows.
    pub primary_node_id: u64,
    /// Replication host of the primary.
    pub primary_host: String,
    /// Replication port of the primary.
    pub primary_port: u16,
}

/// The role view returned by promote/follow (HTTP response body).
#[derive(Clone, Debug)]
pub struct StreamRoleView {
    /// This node's role after the operation.
    pub role: StreamRoleKind,
    /// The node's current stream epoch.
    pub epoch: u64,
    /// Node id of the primary this node is or follows.
    pub primary_node_id: u64,
}

/// Error of a promote/follow request. `bad_request` marks caller errors
/// that map to HTTP 400 (e.g. a missing fencing confirmation).
#[derive(Debug)]
pub struct StreamAdminError {
    /// The underlying error.
    pub error: mudu::error::MuduError,
    /// Whether the failure is a caller error (HTTP 400).
    pub bad_request: bool,
}

impl StreamAdminError {
    fn bad_request(error: mudu::error::MuduError) -> Self {
        Self {
            error,
            bad_request: true,
        }
    }

    fn internal(error: mudu::error::MuduError) -> Self {
        Self {
            error,
            bad_request: false,
        }
    }
}

/// Loads the persisted stream epoch, creating the file with `default_value`
/// on first boot (same load-or-create pattern as the node id).
pub fn load_or_create_stream_epoch<P: AsRef<Path>>(db_path: P, default_value: u64) -> RS<u64> {
    let path = db_path.as_ref().join(STREAM_EPOCH_FILE_NAME);
    if mudu_sys::fs::sync::sync_path_exists(&path) {
        let text = mudu_sys::fs::sync::read_to_string(&path)?;
        text.trim().parse::<u64>().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                format!("decode stream epoch file {} error", path.display()),
                e
            )
        })
    } else {
        store_stream_epoch(db_path, default_value)?;
        Ok(default_value)
    }
}

/// Persists the stream epoch (write + fsync).
pub fn store_stream_epoch<P: AsRef<Path>>(db_path: P, epoch: u64) -> RS<()> {
    let path = db_path.as_ref().join(STREAM_EPOCH_FILE_NAME);
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = options.open(&path)?;
    file.write_all(epoch.to_string().as_bytes())
        .map_err(|e| mudu_error!(ErrorCode::Io, "write stream epoch error", e))?;
    file.sync_data()?;
    Ok(())
}

/// Loads the persisted stream role, `None` on first boot.
pub fn load_stream_role<P: AsRef<Path>>(db_path: P) -> RS<Option<StreamRole>> {
    let path = db_path.as_ref().join(STREAM_ROLE_FILE_NAME);
    if !mudu_sys::fs::sync::sync_path_exists(&path) {
        return Ok(None);
    }
    let bytes = mudu_sys::fs::sync::read(&path)?;
    let role = rmp_serde::from_slice(&bytes).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            format!("decode stream role file {} error", path.display()),
            e
        )
    })?;
    Ok(Some(role))
}

/// Persists the stream role (write + fsync).
pub fn store_stream_role<P: AsRef<Path>>(db_path: P, role: &StreamRole) -> RS<()> {
    let path = db_path.as_ref().join(STREAM_ROLE_FILE_NAME);
    let bytes = rmp_serde::to_vec(role)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode stream role error", e))?;
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = options.open(&path)?;
    file.write_all(&bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "write stream role error", e))?;
    file.sync_data()?;
    Ok(())
}

/// Node-level stream administration entry: everything the promote/follow
/// orchestration needs to reach the node's shared control and its
/// persistence paths.
pub struct StreamAdminEntry {
    /// The node's shared replication control.
    pub ctl: ReplModeCtl,
    /// Database directory (persistence root).
    pub db_path: PathBuf,
    /// This node's cluster id.
    pub node_id: u64,
    /// Server instance (per-worker lookups).
    pub server_instance_id: ServerInstanceId,
    /// Worker count (ack-waiter installation on promote).
    pub worker_count: usize,
    /// This node's replication host (recorded as `primary_host` when this
    /// node is promoted).
    pub self_host: String,
    /// This node's replication port.
    pub repl_port: u16,
    /// This node's client port (leader hints point at the primary's).
    pub client_port: u16,
}

fn stream_admin_registry() -> &'static SMutex<HashMap<u64, Arc<StreamAdminEntry>>> {
    static REGISTRY: OnceLock<SMutex<HashMap<u64, Arc<StreamAdminEntry>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers the node's stream administration entry (server startup,
/// stream mode only).
pub fn register_stream_admin(entry: StreamAdminEntry) -> RS<()> {
    let node_id = entry.node_id;
    stream_admin_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "stream admin registry lock poisoned"))?
        .insert(node_id, Arc::new(entry));
    Ok(())
}

/// Removes the registration (server shutdown).
pub fn unregister_stream_admin(node_id: u64) -> RS<()> {
    stream_admin_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "stream admin registry lock poisoned"))?
        .remove(&node_id);
    Ok(())
}

/// The stream administration entry of the given node (HTTP endpoints).
pub(crate) fn stream_admin_for_node(node_id: u64) -> Option<Arc<StreamAdminEntry>> {
    stream_admin_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&node_id).cloned())
}

/// Promotes this node to stream primary: persists the new epoch first
/// (`max(persisted, known) + 1`), then flips the shared control and
/// records the role. The replication listener already runs, so replicas
/// can subscribe as soon as the control flips. Blocking; the HTTP layer
/// bridges it off the actix runtime.
pub fn stream_promote(node_id: u64, fencing_ok: bool) -> Result<StreamRoleView, StreamAdminError> {
    let entry = stream_admin_for_node(node_id).ok_or_else(|| {
        StreamAdminError::internal(mudu_error!(
            ErrorCode::InvalidState,
            "this server is not running in [replication.stream] mode"
        ))
    })?;
    if entry.ctl.is_primary() {
        return Err(StreamAdminError::bad_request(mudu_error!(
            ErrorCode::InvalidState,
            "this node is already the primary"
        )));
    }
    if !fencing_ok {
        return Err(StreamAdminError::bad_request(mudu_error!(
            ErrorCode::InvalidArgument,
            "promote requires fencing_ok=true: confirm the old primary is fenced (isolated) first"
        )));
    }
    let persisted =
        load_or_create_stream_epoch(&entry.db_path, 1).map_err(StreamAdminError::internal)?;
    let new_epoch = persisted.max(entry.ctl.epoch()) + 1;
    // Epoch first, durably; only then does the role flip open the write
    // gate (a crash between the two keeps the higher epoch reserved).
    store_stream_epoch(&entry.db_path, new_epoch).map_err(StreamAdminError::internal)?;
    entry
        .ctl
        .set_primary(new_epoch)
        .map_err(StreamAdminError::internal)?;
    let role = StreamRole {
        role: StreamRoleKind::Primary,
        epoch: new_epoch,
        primary_node_id: node_id,
        primary_host: entry.self_host.clone(),
        primary_port: entry.repl_port,
    };
    store_stream_role(&entry.db_path, &role).map_err(StreamAdminError::internal)?;
    // Synchronous commit path: the promoted node needs the replica-ack
    // waiter installed (it was only installed at startup for the
    // configured primary).
    if entry.ctl.sync() {
        for worker_index in 0..entry.worker_count as u32 {
            let Ok(handle) = crate::repl_worker(entry.server_instance_id, worker_index) else {
                continue;
            };
            let _ = handle
                .worker()
                .set_repl_ack_waiter(Arc::new(crate::ctl::ReplAckWaiter::new(
                    entry.server_instance_id,
                    worker_index,
                    handle.state().clone(),
                )));
        }
    }
    Ok(StreamRoleView {
        role: StreamRoleKind::Primary,
        epoch: new_epoch,
        primary_node_id: node_id,
    })
}

/// Points this node at a new primary: probes the target (its subscription
/// acceptance must carry the expected node id), records the higher epoch,
/// flips the shared control (the sink reconnects on the next round) and
/// persists the role.
pub async fn stream_follow(
    node_id: u64,
    primary_node_id: u64,
    primary_host: String,
    primary_port: u16,
) -> Result<StreamRoleView, StreamAdminError> {
    let entry = stream_admin_for_node(node_id).ok_or_else(|| {
        StreamAdminError::internal(mudu_error!(
            ErrorCode::InvalidState,
            "this server is not running in [replication.stream] mode"
        ))
    })?;
    if entry.ctl.is_primary() {
        return Err(StreamAdminError::bad_request(mudu_error!(
            ErrorCode::InvalidState,
            "this node is the primary; it cannot follow another primary"
        )));
    }
    if primary_node_id == node_id {
        return Err(StreamAdminError::bad_request(mudu_error!(
            ErrorCode::InvalidArgument,
            "a node cannot follow itself"
        )));
    }
    let primary_epoch = probe_primary_epoch(
        &primary_host,
        primary_port,
        node_id,
        primary_node_id,
        entry.ctl.epoch(),
    )
    .await
    .map_err(StreamAdminError::internal)?;
    let persisted =
        load_or_create_stream_epoch(&entry.db_path, 1).map_err(StreamAdminError::internal)?;
    let epoch = persisted.max(primary_epoch);
    if epoch > persisted {
        store_stream_epoch(&entry.db_path, epoch).map_err(StreamAdminError::internal)?;
    }
    entry.ctl.note_epoch(epoch);
    let hint = mudu_contract::protocol::LeaderHint::new(primary_host.clone(), entry.client_port);
    entry
        .ctl
        .set_follow(
            primary_node_id,
            format!("{}:{}", primary_host, primary_port),
            Some(hint),
        )
        .map_err(StreamAdminError::internal)?;
    let role = StreamRole {
        role: StreamRoleKind::Replica,
        epoch,
        primary_node_id,
        primary_host,
        primary_port,
    };
    store_stream_role(&entry.db_path, &role).map_err(StreamAdminError::internal)?;
    Ok(StreamRoleView {
        role: StreamRoleKind::Replica,
        epoch,
        primary_node_id,
    })
}

/// Probes a would-be primary: subscribes on worker 0 and requires the
/// acceptance frame to carry `expected_node_id`. Returns the source's
/// stream epoch.
async fn probe_primary_epoch(
    host: &str,
    port: u16,
    self_node_id: u64,
    expected_node_id: u64,
    src_epoch: u64,
) -> RS<u64> {
    let addr = format!("{host}:{port}");
    let mut stream = mudu_sys::net::AsyncTcpStream::connect(addr.as_str())
        .await
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("connect primary candidate {addr} error"),
                e
            )
        })?;
    stream
        .set_nodelay(true)
        .map_err(|e| mudu_error!(ErrorCode::Network, "set probe nodelay error", e))?;
    let request = encode_repl_subscribe_request(
        1,
        &ReplSubscribeReq::new_with_epoch(self_node_id, 0, u64::MAX, src_epoch),
    )?;
    mudu_sys::tokio::io::AsyncWriteExt::write_all(&mut stream, &request)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, "write probe subscribe error", e))?;
    let frame = mudu_sys::timeout(FOLLOW_PROBE_TIMEOUT, async {
        let (_stop_tx, mut stop_rx) = mudu_sys::sync::async_::stop_flag::stop_channel();
        let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
        let mut chunk = vec![0u8; 8192];
        crate::read_next_frame(&mut stream, &mut read_buf, &mut chunk, &mut stop_rx).await
    })
    .await
    .ok_or_else(|| {
        mudu_error!(
            ErrorCode::TimedOut,
            format!("primary candidate {addr} probe timed out")
        )
    })??
    .ok_or_else(|| {
        mudu_error!(
            ErrorCode::Network,
            format!("primary candidate {addr} closed before the acceptance frame")
        )
    })?;
    match frame.header().message_type() {
        MessageType::ReplSubscribeAccept => {
            let accept = mudu_contract::protocol::decode_repl_subscribe_accept_frame(&frame)?;
            if accept.node_id() != expected_node_id {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "primary identity mismatch at {addr}: expected node {expected_node_id}, found node {}",
                        accept.node_id()
                    )
                ));
            }
            Ok(accept.epoch())
        }
        MessageType::Error => {
            let error = mudu_contract::protocol::decode_error_response(&frame)?;
            Err(mudu_error!(
                ErrorCode::from_u32(error.code()).unwrap_or(ErrorCode::Internal),
                format!(
                    "primary candidate {addr} rejected the probe: {}",
                    error.message()
                )
            ))
        }
        other => Err(mudu_error!(
            ErrorCode::Parse,
            format!("unexpected probe response frame {other:?} from {addr}")
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn temp_db_dir() -> PathBuf {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("stream-admin-test-{}", mudu_sys::random::uuid_v4()));
        mudu_sys::fs::sync::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn stream_epoch_load_create_and_store() {
        let dir = temp_db_dir();
        // first boot: created with the default
        assert_eq!(load_or_create_stream_epoch(&dir, 1).unwrap(), 1);
        assert_eq!(load_or_create_stream_epoch(&dir, 1).unwrap(), 1);
        // a later promote stores a higher epoch; loads keep it
        store_stream_epoch(&dir, 7).unwrap();
        assert_eq!(load_or_create_stream_epoch(&dir, 1).unwrap(), 7);
    }

    #[test]
    fn stream_role_roundtrip() {
        let dir = temp_db_dir();
        assert!(load_stream_role(&dir).unwrap().is_none());
        let role = StreamRole {
            role: StreamRoleKind::Replica,
            epoch: 4,
            primary_node_id: 2,
            primary_host: "10.0.0.2".to_string(),
            primary_port: 19527,
        };
        store_stream_role(&dir, &role).unwrap();
        assert_eq!(load_stream_role(&dir).unwrap(), Some(role));
    }
}
