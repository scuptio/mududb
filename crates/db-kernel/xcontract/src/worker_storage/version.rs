//! Version helpers shared by the worker-storage commit and write paths.
//!
//! Kept as a leaf module (external deps only) so [`crate::worker_storage`]
//! and its `commit` sub-module can both call these without closing a module
//! cycle.

use ::catalog::contract::data_row::DataRow;
use ::catalog::contract::timestamp::Timestamp;
use ::catalog::contract::version_tuple::VersionTuple;
use mudu::common::result::RS;
use mudu_utils::scoped_task_trace;
use scc::HashMap as SccHashMap;

fn new_value_version(xid: u64, value: Vec<u8>) -> VersionTuple {
    VersionTuple::new(Timestamp::new(xid, u64::MAX), value)
}

/// Writes one committed KV version into the store, appending the version to
/// the key's row history.
pub async fn write_version_to_kv_store_async(
    kv_store: &SccHashMap<Vec<u8>, DataRow>,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    xid: u64,
) -> RS<()> {
    scoped_task_trace!();
    let row = kv_store
        .get_async(&key)
        .await
        .map(|entry| entry.get().clone())
        .unwrap_or_else(|| DataRow::new(0));
    let version = match value {
        Some(value) => new_value_version(xid, value),
        None => VersionTuple::new_delete(Timestamp::new(xid, u64::MAX)),
    };
    row.write(version, None).await?;
    let _ = kv_store.insert_async(key, row).await;
    Ok(())
}

/// The latest committed version of one key row, when one exists.
pub async fn latest_version(row: &DataRow) -> Option<VersionTuple> {
    row.read_latest().await.ok().flatten()
}
