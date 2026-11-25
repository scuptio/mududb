//! Canonical guest facade `mududb.cache`: the volatile cache handle.

use crate::sys_interface::sync_api;
use mudu::common::id::OID;
use mudu::common::result::RS;

/// A volatile cache handle; obtain one with [`Cache::open`].
pub struct Cache {
    id: OID,
}

impl Cache {
    /// Open a volatile cache by name (declared with `CREATE CACHE ... LIKE
    /// <table>`), returning its handle.
    pub fn open(name: &str) -> RS<Cache> {
        Ok(Cache {
            id: sync_api::mudu_cache_open(name)?,
        })
    }

    /// The cache handle (catalog OID).
    pub fn id(&self) -> OID {
        self.id
    }

    /// Read a cache entry.
    pub fn get(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        sync_api::mudu_cache_get(self.id, key)
    }

    /// Write a cache entry with an optional TTL in seconds.
    pub fn put(&self, key: &[u8], value: &[u8], ttl_secs: Option<u64>) -> RS<()> {
        sync_api::mudu_cache_put(self.id, key, value, ttl_secs)
    }

    /// Remove a cache entry.
    pub fn delete(&self, key: &[u8]) -> RS<()> {
        sync_api::mudu_cache_delete(self.id, key)
    }

    /// Read a cache entry's remaining TTL in seconds.
    pub fn ttl(&self, key: &[u8]) -> RS<Option<u64>> {
        sync_api::mudu_cache_ttl(self.id, key)
    }
}
