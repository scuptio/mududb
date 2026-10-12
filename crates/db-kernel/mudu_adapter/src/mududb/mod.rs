//! Remote Mudud protocol backend implementation.

mod async_api;
mod async_loop;
mod fs_stubs;
mod sync_api;
#[cfg(all(test, not(miri)))]
mod tests;
mod topology;

pub use async_api::{
    mudu_batch_async, mudu_close_async, mudu_command_async, mudu_get_async, mudu_open_async,
    mudu_put_async, mudu_query_async, mudu_range_async,
};
pub use fs_stubs::{
    mudu_fs_close, mudu_fs_close_async, mudu_fs_fstat, mudu_fs_fstat_async, mudu_fs_fsync,
    mudu_fs_fsync_async, mudu_fs_lseek, mudu_fs_lseek_async, mudu_fs_open, mudu_fs_open_async,
    mudu_fs_pread, mudu_fs_pread_async, mudu_fs_pwrite, mudu_fs_pwrite_async, mudu_fs_read,
    mudu_fs_read_async, mudu_fs_readdir, mudu_fs_readdir_async, mudu_fs_stat, mudu_fs_stat_async,
    mudu_fs_write, mudu_fs_write_async,
};
pub use sync_api::{
    mudu_batch, mudu_close, mudu_command, mudu_get, mudu_open, mudu_put, mudu_query, mudu_range,
};

use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_client::client::async_client::AsyncClientImpl;
use mudu_client::client::client::SyncClient;
use mudu_client::management::ServerTopology;
use mudu_sys::sync::SMutex;
use mudu_sys::sync::async_::mutex::AMutex;
use mudu_sys::sync::async_::rwlock::ARwLock;
use scc::HashMap as SccHashMap;
use std::collections::HashMap;
use std::sync::Arc;

struct MududSession {
    client: SyncClient,
    remote_session_id: u128,
}

type SessionRef = Arc<SMutex<MududSession>>;

lazy_static! {
    static ref SESSIONS: SccHashMap<OID, SessionRef> = SccHashMap::new();
    static ref ASYNC_NATIVE_SESSIONS: ARwLock<HashMap<OID, Arc<AMutex<AsyncMududSession>>>> =
        ARwLock::new(HashMap::new());
    static ref WORKER_TOPOLOGIES: SccHashMap<String, ServerTopology> = SccHashMap::new();
}

struct AsyncMududSession {
    client: AsyncClientImpl,
    remote_session_id: u128,
}

fn with_session<R, F>(session_id: OID, f: F) -> RS<R>
where
    F: FnOnce(&mut MududSession) -> RS<R>,
{
    // Clone the session reference out of the map and drop the scc entry
    // immediately. An scc entry holds its bucket lock, so keeping it alive
    // across `f` (a blocking network call) serializes every session hashing
    // to the same bucket and can deadlock: one thread blocks in `f` waiting
    // on a server-side lock held by another session, whose thread in turn
    // blocks acquiring the same bucket lock.
    let session_ref = {
        let entry = SESSIONS.get_sync(&session_id).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("session {} does not exist", session_id)
            )
        })?;
        entry.get().clone()
    };
    let mut session = session_ref
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "mudud session lock poisoned"))?;
    f(&mut session)
}

async fn async_session(session_id: OID) -> RS<Arc<AMutex<AsyncMududSession>>> {
    ASYNC_NATIVE_SESSIONS
        .read()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("session {} does not exist", session_id)
            )
        })
}

fn session_open_config_json(worker_id: OID) -> Option<String> {
    if worker_id == 0 {
        None
    } else {
        Some(
            serde_json::json!({
                "session_id": UniOid::from(0),
                "worker_id": UniOid::from(worker_id),
            })
            .to_string(),
        )
    }
}
