use crate::contract::timestamp::Timestamp;
use crate::contract::version_tuple::VersionTuple;
use crate::x_log::worker_kv_log::WorkerKvLog;
use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvItem {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerSnapshot {
    xid: u64,
    running: Vec<u64>,
}

#[derive(Clone)]
pub struct WorkerKvStore {
    worker_id: usize,
    inner: Arc<Mutex<WorkerKvState>>,
    log: WorkerKvLog,
}

#[derive(Default)]
struct WorkerKvState {
    rows: BTreeMap<Vec<u8>, Vec<VersionTuple>>,
    snapshot_mgr: WorkerSnapshotMgr,
}

#[derive(Default)]
struct WorkerSnapshotMgr {
    next_ts: u64,
    running: Vec<u64>,
}

impl WorkerKvStore {
    pub fn new(worker_id: usize, log: WorkerKvLog) -> Self {
        Self {
            worker_id,
            inner: Arc::new(Mutex::new(WorkerKvState::default())),
            log,
        }
    }

    pub fn worker_id(&self) -> usize {
        self.worker_id
    }

    pub fn begin_tx(&self) -> RS<WorkerSnapshot> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv store lock poisoned"))?;
        Ok(guard.snapshot_mgr.begin_tx())
    }

    pub fn rollback_tx(&self, xid: u64) -> RS<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv store lock poisoned"))?;
        guard.snapshot_mgr.end_tx(xid)
    }

    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> RS<Option<Vec<u8>>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv store lock poisoned"))?;
        Ok(guard.get_latest(key.as_ref()))
    }

    pub fn get_with_snapshot<K: AsRef<[u8]>>(
        &self,
        snapshot: &WorkerSnapshot,
        key: K,
    ) -> RS<Option<Vec<u8>>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv store lock poisoned"))?;
        Ok(guard.get_with_snapshot(snapshot, key.as_ref()))
    }

    pub fn put<K: Into<Buf>, V: Into<Buf>>(&self, key: K, value: V) -> RS<()> {
        let key = key.into();
        let value = value.into();
        self.log.append_put(&key, &value)?;
        self.put_local(key, value)
    }

    pub fn put_local<K: Into<Buf>, V: Into<Buf>>(&self, key: K, value: V) -> RS<()> {
        let key = key.into();
        let value = value.into();
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv store lock poisoned"))?;
        let commit_ts = guard.snapshot_mgr.alloc_committed_ts();
        guard.put_version(key, value, commit_ts);
        Ok(())
    }

    pub fn commit_put_batch(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> RS<()> {
        if items.is_empty() {
            return self.rollback_tx(xid);
        }
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv store lock poisoned"))?;
        for (key, _) in &items {
            if guard.has_write_conflict(snapshot, key) {
                guard.snapshot_mgr.end_tx(xid)?;
                return Err(m_error!(
                    EC::TxErr,
                    format!(
                        "write-write conflict on key {:?} for transaction {}",
                        String::from_utf8_lossy(key),
                        xid
                    )
                ));
            }
        }
        let mut payload = Vec::new();
        for (key, value) in &items {
            payload.extend_from_slice(&WorkerKvLog::encode_put_record(key, value));
        }
        self.log.append_raw(&payload)?;
        self.log.flush()?;
        for (key, value) in items {
            guard.put_version(key, value, xid);
        }
        guard.snapshot_mgr.end_tx(xid)?;
        Ok(())
    }

    pub fn range_scan<S: AsRef<[u8]>, E: AsRef<[u8]>>(
        &self,
        start_key: S,
        end_key: E,
    ) -> RS<Vec<KvItem>> {
        self.range_scan_internal(None, start_key.as_ref(), end_key.as_ref())
    }

    pub fn range_scan_with_snapshot<S: AsRef<[u8]>, E: AsRef<[u8]>>(
        &self,
        snapshot: &WorkerSnapshot,
        start_key: S,
        end_key: E,
    ) -> RS<Vec<KvItem>> {
        self.range_scan_internal(Some(snapshot), start_key.as_ref(), end_key.as_ref())
    }

    fn range_scan_internal(
        &self,
        snapshot: Option<&WorkerSnapshot>,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        let start_key = start_key.to_vec();
        let end_key = end_key.to_vec();
        let guard = self
            .inner
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv store lock poisoned"))?;
        let iter = if end_key.is_empty() {
            guard.rows.range((Included(start_key), Unbounded))
        } else {
            guard.rows.range((Included(start_key), Excluded(end_key)))
        };
        Ok(iter
            .filter_map(|(key, versions)| {
                let visible = match snapshot {
                    Some(snapshot) => latest_visible_version_for_snapshot(versions, snapshot),
                    None => latest_version(versions),
                }?;
                Some(KvItem {
                    key: key.clone(),
                    value: visible.tuple().clone(),
                })
            })
            .collect())
    }
}

impl WorkerSnapshot {
    pub fn xid(&self) -> u64 {
        self.xid
    }
}

impl WorkerKvState {
    fn get_latest(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.rows
            .get(key)
            .and_then(|versions| latest_version(versions))
            .map(|version| version.tuple().clone())
    }

    fn get_with_snapshot(&self, snapshot: &WorkerSnapshot, key: &[u8]) -> Option<Vec<u8>> {
        self.rows
            .get(key)
            .and_then(|versions| latest_visible_version_for_snapshot(versions, snapshot))
            .map(|version| version.tuple().clone())
    }

    fn put_version(&mut self, key: Vec<u8>, value: Vec<u8>, commit_ts: u64) {
        self.rows.entry(key).or_default().push(VersionTuple::new(
            Timestamp::new(commit_ts, u64::MAX),
            value,
        ));
    }

    fn has_write_conflict(&self, snapshot: &WorkerSnapshot, key: &[u8]) -> bool {
        self.rows
            .get(key)
            .and_then(|versions| latest_version(versions))
            .map(|latest| !is_visible_to_snapshot(latest.timestamp().c_min(), snapshot))
            .unwrap_or(false)
    }
}

impl WorkerSnapshotMgr {
    fn begin_tx(&mut self) -> WorkerSnapshot {
        self.next_ts += 1;
        let xid = self.next_ts;
        let snapshot = WorkerSnapshot {
            xid,
            running: self.running.clone(),
        };
        insert_sorted_unique(&mut self.running, xid);
        snapshot
    }

    fn alloc_committed_ts(&mut self) -> u64 {
        self.next_ts += 1;
        self.next_ts
    }

    fn end_tx(&mut self, xid: u64) -> RS<()> {
        match self.running.binary_search(&xid) {
            Ok(index) => {
                self.running.remove(index);
                Ok(())
            }
            Err(_) => Err(m_error!(
                EC::NoSuchElement,
                format!("transaction {} is not active", xid)
            )),
        }
    }
}

fn latest_version(versions: &[VersionTuple]) -> Option<&VersionTuple> {
    versions.last()
}

fn latest_visible_version_for_snapshot<'a>(
    versions: &'a [VersionTuple],
    snapshot: &WorkerSnapshot,
) -> Option<&'a VersionTuple> {
    versions
        .iter()
        .rev()
        .find(|version| is_visible_to_snapshot(version.timestamp().c_min(), snapshot))
}

fn is_visible_to_snapshot(version_xid: u64, snapshot: &WorkerSnapshot) -> bool {
    if version_xid > snapshot.xid {
        return false;
    }
    snapshot.running.binary_search(&version_xid).is_err()
}

fn insert_sorted_unique(values: &mut Vec<u64>, value: u64) {
    match values.binary_search(&value) {
        Ok(_) => {}
        Err(index) => values.insert(index, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x_log::worker_kv_log::{WorkerKvLog, WorkerLogLayout};
    use mudu::common::id::gen_oid;
    use std::env::temp_dir;

    fn test_store(prefix: &str) -> WorkerKvStore {
        let dir = temp_dir().join(format!("{}_{}", prefix, gen_oid()));
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
        let log = WorkerKvLog::new(layout).unwrap();
        WorkerKvStore::new(1, log)
    }

    #[test]
    fn worker_store_persists_local_state() {
        let store = test_store("worker_kv_store_test");
        store.put(b"a".to_vec(), b"1".to_vec()).unwrap();
        store.put(b"b".to_vec(), b"2".to_vec()).unwrap();
        assert_eq!(store.get(b"a").unwrap(), Some(b"1".to_vec()));
        let rows = store.range_scan(b"a", b"c").unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn worker_store_snapshot_does_not_see_later_commit() {
        let store = test_store("worker_kv_store_snapshot");
        store.put(b"a".to_vec(), b"0".to_vec()).unwrap();
        let snapshot = store.begin_tx().unwrap();
        store.put(b"a".to_vec(), b"1".to_vec()).unwrap();

        assert_eq!(
            store.get_with_snapshot(&snapshot, b"a").unwrap(),
            Some(b"0".to_vec())
        );
        assert_eq!(store.get(b"a").unwrap(), Some(b"1".to_vec()));
    }

    #[test]
    fn worker_store_snapshot_range_is_stable() {
        let store = test_store("worker_kv_store_range_snapshot");
        store.put(b"a".to_vec(), b"1".to_vec()).unwrap();
        let snapshot = store.begin_tx().unwrap();
        store.put(b"b".to_vec(), b"2".to_vec()).unwrap();
        store.put(b"c".to_vec(), b"3".to_vec()).unwrap();

        let rows = store
            .range_scan_with_snapshot(&snapshot, b"a", b"z")
            .unwrap();
        assert_eq!(
            rows,
            vec![KvItem {
                key: b"a".to_vec(),
                value: b"1".to_vec()
            }]
        );
    }

    #[test]
    fn worker_store_commit_makes_tx_visible_to_future_snapshots() {
        let store = test_store("worker_kv_store_commit_visible");
        let first = store.begin_tx().unwrap();
        store
            .commit_put_batch(&first, first.xid(), vec![(b"a".to_vec(), b"1".to_vec())])
            .unwrap();
        let later = store.begin_tx().unwrap();

        assert_eq!(
            store.get_with_snapshot(&later, b"a").unwrap(),
            Some(b"1".to_vec())
        );
    }

    #[test]
    fn worker_store_rollback_keeps_previous_visible_version() {
        let store = test_store("worker_kv_store_rollback_visible");
        store.put(b"a".to_vec(), b"base".to_vec()).unwrap();
        let snapshot = store.begin_tx().unwrap();
        store.rollback_tx(snapshot.xid()).unwrap();

        assert_eq!(store.get(b"a").unwrap(), Some(b"base".to_vec()));
    }

    #[test]
    fn worker_store_multiple_versions_choose_latest_visible() {
        let store = test_store("worker_kv_store_multiversion");
        store.put(b"a".to_vec(), b"v0".to_vec()).unwrap();
        let old_snapshot = store.begin_tx().unwrap();
        store.put(b"a".to_vec(), b"v1".to_vec()).unwrap();
        let new_snapshot = store.begin_tx().unwrap();

        assert_eq!(
            store.get_with_snapshot(&old_snapshot, b"a").unwrap(),
            Some(b"v0".to_vec())
        );
        assert_eq!(
            store.get_with_snapshot(&new_snapshot, b"a").unwrap(),
            Some(b"v1".to_vec())
        );
    }

    #[test]
    fn worker_store_first_committer_wins_on_same_key() {
        let store = test_store("worker_kv_store_first_committer");
        store.put(b"a".to_vec(), b"base".to_vec()).unwrap();

        let tx1 = store.begin_tx().unwrap();
        let tx2 = store.begin_tx().unwrap();

        store
            .commit_put_batch(&tx1, tx1.xid(), vec![(b"a".to_vec(), b"v1".to_vec())])
            .unwrap();
        let err = store
            .commit_put_batch(&tx2, tx2.xid(), vec![(b"a".to_vec(), b"v2".to_vec())])
            .unwrap_err();

        assert!(err.to_string().contains("write-write conflict"));
        assert_eq!(store.get(b"a").unwrap(), Some(b"v1".to_vec()));
    }

    #[test]
    fn worker_store_allows_concurrent_commits_on_different_keys() {
        let store = test_store("worker_kv_store_disjoint_keys");
        let tx1 = store.begin_tx().unwrap();
        let tx2 = store.begin_tx().unwrap();

        store
            .commit_put_batch(&tx1, tx1.xid(), vec![(b"a".to_vec(), b"v1".to_vec())])
            .unwrap();
        store
            .commit_put_batch(&tx2, tx2.xid(), vec![(b"b".to_vec(), b"v2".to_vec())])
            .unwrap();

        assert_eq!(store.get(b"a").unwrap(), Some(b"v1".to_vec()));
        assert_eq!(store.get(b"b").unwrap(), Some(b"v2".to_vec()));
    }
}
