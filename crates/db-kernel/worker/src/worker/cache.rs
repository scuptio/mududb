//! Volatile, non-replicated in-memory cache: the worker-local store backing
//! the `cache-open` / `cache-get` / `cache-put` / `cache-delete` /
//! `cache-ttl` syscalls.
//!
//! A cache is a per-worker `HashMap` keyed by the cache's catalog OID. It
//! never touches storage, WAL, checkpoint, or replication: entries live in
//! process memory and are dropped on restart, so they must always be
//! reconstructible from the authoritative table (see the game object model
//! design). TTL eviction is lazy: an expired entry is dropped on access and
//! otherwise lingers until the next access or overwrite.

use super::*;
use catalog::contract::app_schema::current_schema;
use catalog::contract::schema_class::SchemaClassKind;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_sys::time::system_time_now;
use std::collections::HashMap;

/// One cache entry: the stored bytes and an absolute expiry timestamp in
/// seconds (`None` = never expires).
struct CacheEntry {
    value: Vec<u8>,
    expire_at: Option<u64>,
}

/// One cache instance: its entries keyed by the raw cache key bytes.
#[derive(Default)]
struct CacheInstance {
    entries: HashMap<Vec<u8>, CacheEntry>,
    /// Default entry TTL from the `CREATE CACHE ... TTL <n>` declaration;
    /// applied to puts that carry no per-call TTL.
    default_ttl_secs: Option<u64>,
}

/// The worker-local cache store: cache OID -> instance.
#[derive(Default)]
pub struct CacheStore {
    inner: SMutex<HashMap<OID, CacheInstance>>,
}

impl CacheStore {
    /// Opens `cache`, registering its declared default TTL: a fresh instance
    /// starts empty, a reopen keeps the existing (volatile) contents and
    /// refreshes the default TTL from the catalog.
    pub fn open(&self, cache: OID, default_ttl_secs: Option<u64>) -> RS<()> {
        let mut guard = self.inner.lock()?;
        let instance = guard.entry(cache).or_default();
        instance.default_ttl_secs = default_ttl_secs;
        Ok(())
    }

    /// Returns the value of `key` in `cache`, dropping an expired entry.
    pub fn get(&self, cache: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        let now = now_secs();
        let mut guard = self.inner.lock()?;
        let Some(instance) = guard.get_mut(&cache) else {
            return Ok(None);
        };
        let expired = instance
            .entries
            .get(key)
            .is_some_and(|entry| entry.expire_at.is_some_and(|at| at <= now));
        if expired {
            instance.entries.remove(key);
            return Ok(None);
        }
        Ok(instance.entries.get(key).map(|entry| entry.value.clone()))
    }

    /// Inserts or updates `key` in `cache`. `ttl_secs` overrides the cache's
    /// declared default TTL; `None` applies the default (if any).
    pub fn put(&self, cache: OID, key: Vec<u8>, value: Vec<u8>, ttl_secs: Option<u64>) -> RS<()> {
        let now = now_secs();
        let mut guard = self.inner.lock()?;
        let instance = guard.entry(cache).or_default();
        let expire_at = ttl_secs
            .or(instance.default_ttl_secs)
            .map(|secs| now.saturating_add(secs));
        instance
            .entries
            .insert(key, CacheEntry { value, expire_at });
        Ok(())
    }

    /// Removes `key` from `cache`, returning whether an entry was removed.
    pub fn delete(&self, cache: OID, key: &[u8]) -> RS<bool> {
        let mut guard = self.inner.lock()?;
        match guard.get_mut(&cache) {
            Some(instance) => Ok(instance.entries.remove(key).is_some()),
            None => Ok(false),
        }
    }

    /// Returns the remaining TTL of `key` in `cache` in seconds; `None` means
    /// the entry does not expire (or does not exist).
    pub fn ttl(&self, cache: OID, key: &[u8]) -> RS<Option<u64>> {
        let now = now_secs();
        let mut guard = self.inner.lock()?;
        let Some(instance) = guard.get_mut(&cache) else {
            return Ok(None);
        };
        let Some(entry) = instance.entries.get(key) else {
            return Ok(None);
        };
        let Some(expire_at) = entry.expire_at else {
            return Ok(None);
        };
        if expire_at <= now {
            instance.entries.remove(key);
            return Ok(None);
        }
        Ok(Some(expire_at - now))
    }
}

impl WorkerRuntime {
    /// Opens a cache by name, returning its catalog OID as the handle. The
    /// cache must have been declared with `CREATE CACHE ... LIKE <table>`.
    /// The bare name resolves through the caller's application schema (the
    /// same search path as SQL and the relation syscalls: the app schema
    /// first, then the default schema). A fresh in-memory instance is created
    /// on first open with the declared default TTL; a reopen returns the same
    /// handle and keeps the existing (volatile) contents.
    pub async fn cache_open(&self, name: &str, app_name: Option<&str>) -> RS<OID> {
        let desc = self
            .meta_mgr()
            .get_table_by_name(&current_schema(app_name), name)
            .await?
            .ok_or_else(|| {
                mudu_error!(ErrorCode::ObjectNotFound, format!("no such cache {name}"))
            })?;
        if desc.kind() != SchemaClassKind::Cache {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("{name} is not a cache (CREATE CACHE ... LIKE <table>)")
            ));
        }
        let oid = desc.id();
        self.cache_store.open(oid, desc.ttl_secs())?;
        Ok(oid)
    }

    /// Reads a cache entry (see [`CacheStore::get`]).
    pub fn cache_get(&self, cache: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.cache_store.get(cache, key)
    }

    /// Writes a cache entry (see [`CacheStore::put`]).
    pub fn cache_put(
        &self,
        cache: OID,
        key: Vec<u8>,
        value: Vec<u8>,
        ttl_secs: Option<u64>,
    ) -> RS<()> {
        self.cache_store.put(cache, key, value, ttl_secs)
    }

    /// Removes a cache entry (see [`CacheStore::delete`]).
    pub fn cache_delete(&self, cache: OID, key: &[u8]) -> RS<bool> {
        self.cache_store.delete(cache, key)
    }

    /// Reads a cache entry's remaining TTL (see [`CacheStore::ttl`]).
    pub fn cache_ttl(&self, cache: OID, key: &[u8]) -> RS<Option<u64>> {
        self.cache_store.ttl(cache, key)
    }
}

/// The current wall-clock time in whole seconds since the Unix epoch.
fn now_secs() -> u64 {
    system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_store_put_get_delete_roundtrip() {
        let store = CacheStore::default();
        store.put(1, b"k".to_vec(), b"v".to_vec(), None).unwrap();
        assert_eq!(store.get(1, b"k").unwrap(), Some(b"v".to_vec()));
        assert!(store.delete(1, b"k").unwrap());
        assert_eq!(store.get(1, b"k").unwrap(), None);
    }

    #[test]
    fn cache_store_ttl_expires_entry() {
        let store = CacheStore::default();
        store.put(2, b"k".to_vec(), b"v".to_vec(), Some(1)).unwrap();
        assert_eq!(store.ttl(2, b"k").unwrap(), Some(1));
        // A zero-second TTL has already expired by the time it is read.
        store.put(3, b"k".to_vec(), b"v".to_vec(), Some(0)).unwrap();
        assert_eq!(store.get(3, b"k").unwrap(), None);
        assert_eq!(store.ttl(3, b"k").unwrap(), None);
    }

    #[test]
    fn cache_store_caches_are_isolated() {
        let store = CacheStore::default();
        store.put(1, b"k".to_vec(), b"a".to_vec(), None).unwrap();
        store.put(2, b"k".to_vec(), b"b".to_vec(), None).unwrap();
        assert_eq!(store.get(1, b"k").unwrap(), Some(b"a".to_vec()));
        assert_eq!(store.get(2, b"k").unwrap(), Some(b"b".to_vec()));
    }

    #[test]
    fn cache_store_open_registers_default_ttl() {
        let store = CacheStore::default();
        // A put without a per-call TTL on an unopened cache never expires.
        store.put(1, b"k".to_vec(), b"v".to_vec(), None).unwrap();
        assert_eq!(store.ttl(1, b"k").unwrap(), None);
        // Opening registers the declared default TTL; existing contents stay.
        store.open(1, Some(60)).unwrap();
        assert_eq!(store.get(1, b"k").unwrap(), Some(b"v".to_vec()));
        // New puts without a per-call TTL pick up the default.
        store.put(1, b"k2".to_vec(), b"v2".to_vec(), None).unwrap();
        assert_eq!(store.ttl(1, b"k2").unwrap(), Some(60));
        // A per-call TTL still overrides the default.
        store
            .put(1, b"k3".to_vec(), b"v3".to_vec(), Some(10))
            .unwrap();
        assert_eq!(store.ttl(1, b"k3").unwrap(), Some(10));
        // A reopen refreshes the default TTL and keeps the contents.
        store.open(1, Some(30)).unwrap();
        store.put(1, b"k4".to_vec(), b"v4".to_vec(), None).unwrap();
        assert_eq!(store.ttl(1, b"k4").unwrap(), Some(30));
    }

    #[test]
    fn cache_store_default_ttl_expires_entry() {
        let store = CacheStore::default();
        store.open(2, Some(0)).unwrap();
        // The zero-second default expires the entry by the time it is read.
        store.put(2, b"k".to_vec(), b"v".to_vec(), None).unwrap();
        assert_eq!(store.get(2, b"k").unwrap(), None);
    }
}
