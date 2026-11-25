//! Receiver-side dedup cache of the partition RPC service.
//!
//! A replayed request (same `(src, request_id)`) is answered from the cache
//! instead of re-executing. Kept as a leaf module (dependencies are external
//! or leaf only) so [`crate::contract::WorkerXContract`] can hold it without
//! pulling the RPC business modules into the module cycle.

use crate::partition_rpc::PartitionRpcResponse;
use mudu::channel::RequestId;
use mudu::common::id::OID;
use mudu_sys::sync::SMutex;
use mudu_utils::channel::channel_now_ms;
use std::collections::HashMap;

/// Time-to-live of one dedup cache entry.
pub(crate) const RPC_DEDUP_TTL_MS: u64 = 60_000;
/// Upper bound of dedup entries; past the cap new responses are simply not
/// cached (a replay then re-executes, which retry policy only does for
/// idempotent requests).
pub(crate) const RPC_DEDUP_MAX_ENTRIES: usize = 4096;

/// Receiver-side dedup cache of the partition RPC service: a replayed
/// request (same `(src, request_id)`) is answered from the cache instead of
/// re-executing.
pub(crate) struct RpcDedupCache {
    entries: SMutex<HashMap<(OID, RequestId), (u64, PartitionRpcResponse)>>,
}

impl RpcDedupCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: SMutex::new(HashMap::new()),
        }
    }

    pub(crate) fn lookup(&self, src: OID, request_id: RequestId) -> Option<PartitionRpcResponse> {
        let mut entries = self.entries.lock().ok()?;
        let expired = entries
            .get(&(src, request_id))
            .is_some_and(|(expire_ms, _)| channel_now_ms() >= *expire_ms);
        if expired {
            entries.remove(&(src, request_id));
            return None;
        }
        entries
            .get(&(src, request_id))
            .map(|(_, response)| response.clone())
    }

    pub(crate) fn store(&self, src: OID, request_id: RequestId, response: PartitionRpcResponse) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let now = channel_now_ms();
        entries.retain(|_, (expire_ms, _)| now < *expire_ms);
        if entries.len() >= RPC_DEDUP_MAX_ENTRIES {
            tracing::warn!("partition rpc dedup cache full; response not cached");
            return;
        }
        entries.insert(
            (src, request_id),
            (now.saturating_add(RPC_DEDUP_TTL_MS), response),
        );
    }
}
