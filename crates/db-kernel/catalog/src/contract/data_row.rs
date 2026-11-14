use crate::contract::snapshot::Snapshot;
use crate::contract::timestamp::Timestamp;
use crate::contract::version_delta::VersionDelta;
use crate::contract::version_tuple::VersionTuple;
use mudu::common::id::{TupleID, OID};
use mudu::common::result::RS;
use mudu::common::update_delta::UpdateDelta;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::async_::AMutex;
use mudu_utils::scoped_task_trace;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const UNCOMPRESSED_VERSION_COUNT: usize = 4;

/// A multi-versioned row: a small window of the newest full versions plus an
/// append-only delta chain used to reconstruct older versions.
#[derive(Clone)]
pub struct DataRow {
    inner: Arc<AMutex<DataRowInner>>,
}

struct DataRowInner {
    tid: TupleID,
    bytes: usize,
    accounting: Option<Arc<AtomicUsize>>,
    // Full versions are stored from oldest to newest inside the retained
    // in-memory window. New commits therefore append to the tail, and the
    // latest version is always `tuple.last()`.
    //
    // Only the newest `UNCOMPRESSED_VERSION_COUNT` full versions stay in this
    // window. When the window overflows, the oldest retained full version is
    // evicted from the head.
    tuple: Vec<VersionTuple>,
    // Delta entries are append-only and ordered from oldest transition to
    // newest transition.
    //
    // Each `delta[i]` converts a newer version into the immediately previous
    // older version. For example, the logical chain `v1 <- v2 <- v3` is stored
    // as `[v2->v1, v3->v2]`.
    //
    // The delta chain covers the entire history except for the oldest version.
    // It also keeps transitions for versions that are still present in `tuple`,
    // so the chain remains contiguous after older full versions are evicted
    // from the retained window.
    delta: Vec<VersionDelta>,
}

impl DataRowInner {
    fn new(tid: TupleID) -> Self {
        Self {
            tid,
            bytes: std::mem::size_of::<Self>(),
            accounting: None,
            tuple: vec![],
            delta: vec![],
        }
    }
}

impl DataRowInner {
    fn write_version(
        &mut self,
        version: VersionTuple,
        prev_version: Option<VersionDelta>,
    ) -> RS<()> {
        if let Some(latest) = self.tuple.last() {
            let delta = prev_version.unwrap_or_else(|| build_version_delta(&version, latest));
            self.append_delta(delta);
        }
        self.push_version(version);
        Ok(())
    }

    /// Appends `version` without cloning the previous version's payload into
    /// the delta chain. The recorded delta keeps the previous version's
    /// timestamp / deleted flag (so reconstructed versions stay
    /// visibility-correct) but no payload bytes, so the append-only delta
    /// chain does not pin every historical payload. Versions reconstructed
    /// through such deltas therefore carry stale payload bytes; readers that
    /// need the payload must treat delta-walk results as metadata-only (see
    /// [`DataRowInner::read_version_detailed`]).
    fn write_version_shallow(&mut self, version: VersionTuple) -> RS<()> {
        if let Some(latest) = self.tuple.last() {
            let delta = VersionDelta::new(
                latest.timestamp().clone(),
                latest.is_deleted(),
                vec![UpdateDelta::new(0, 0, Vec::new())],
            );
            self.append_delta(delta);
        }
        self.push_version(version);
        Ok(())
    }

    fn append_delta(&mut self, delta: VersionDelta) {
        let size = 64
            + delta
                .update_delta()
                .iter()
                .map(|d| d.delta().len() + 24)
                .sum::<usize>();
        self.add_bytes(size);
        self.delta.push(delta);
    }

    fn add_bytes(&mut self, amount: usize) {
        self.bytes += amount;
        if let Some(account) = &self.accounting {
            account.fetch_add(amount, Ordering::Relaxed);
        }
    }

    fn push_version(&mut self, version: VersionTuple) {
        self.add_bytes(version.tuple().len() + 64);
        self.tuple.push(version);
        if self.tuple.len() > UNCOMPRESSED_VERSION_COUNT {
            let removed = self.tuple.remove(0).tuple().len() + 64;
            self.bytes -= removed;
            if let Some(account) = &self.accounting {
                account.fetch_sub(removed, Ordering::Relaxed);
            }
        }
    }

    fn read_latest(&self) -> RS<Option<VersionTuple>> {
        Ok(self.tuple.last().cloned())
    }

    fn read_version(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        Ok(self
            .read_version_detailed(snapshot)?
            .map(|(version, _)| version))
    }

    /// Reads the visible version like [`DataRowInner::read_version`], and
    /// additionally reports whether the returned payload bytes are the
    /// version's own. The flag is true only when the visible version was
    /// found inside the retained full-version window with a non-empty
    /// payload; versions reconstructed through the delta chain always report
    /// false, because shallow writes (see
    /// [`DataRowInner::write_version_shallow`]) do not store payload bytes in
    /// the delta chain.
    fn read_version_detailed(&self, snapshot: &Snapshot) -> RS<Option<(VersionTuple, bool)>> {
        if let Some(version) = self
            .tuple
            .iter()
            .rev()
            .find(|v| snapshot.is_tuple_visible(v.timestamp()))
            .cloned()
        {
            let payload_authoritative = !version.tuple().is_empty();
            return Ok(Some((version, payload_authoritative)));
        }

        let Some(mut version) = self.tuple.first().cloned() else {
            return Ok(None);
        };

        let older_version_count = self
            .delta
            .len()
            .saturating_add(1)
            .saturating_sub(self.tuple.len());
        if older_version_count == 0 {
            return Ok(None);
        }

        let start = older_version_count - 1;
        for index in (0..=start).rev() {
            apply_version_delta(&mut version, &self.delta[index]);
            if snapshot.is_tuple_visible(version.timestamp()) {
                return Ok(Some((version, false)));
            }
        }

        Ok(None)
    }
}

impl DataRow {
    /// Creates an empty row with the given tuple id.
    pub fn new(tid: TupleID) -> Self {
        Self {
            inner: Arc::new(AMutex::new(DataRowInner::new(tid))),
        }
    }

    /// The tuple id of this row.
    pub async fn tuple_id(&self) -> RS<Option<OID>> {
        Ok(Some(self.inner.lock().await.tid as OID))
    }

    /// Reads the newest version visible to `snapshot` without blocking an executor.
    pub async fn read(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        self.inner.lock().await.read_version(snapshot)
    }

    /// Reads the visible version and whether its payload is authoritative.
    pub async fn read_detailed(&self, snapshot: &Snapshot) -> RS<Option<(VersionTuple, bool)>> {
        self.inner.lock().await.read_version_detailed(snapshot)
    }

    /// Reads the latest committed version regardless of snapshot visibility.
    pub async fn read_latest(&self) -> RS<Option<VersionTuple>> {
        self.inner.lock().await.read_latest()
    }

    /// Publishes a version and its optional compensating delta under the async row lock.
    pub async fn write(&self, version: VersionTuple, prev_version: Option<VersionDelta>) -> RS<()> {
        scoped_task_trace!();
        self.inner.lock().await.write_version(version, prev_version)
    }

    /// Publishes a version retaining only metadata in the historical delta chain.
    pub async fn write_shallow(&self, version: VersionTuple) -> RS<()> {
        self.inner.lock().await.write_version_shallow(version)
    }

    /// Atomically reads the latest committed version, computes a new payload
    /// from it, and appends that payload as a new version — all under the row
    /// lock. Used by the deferred (lock-free) delta apply: the read-compute-
    /// append sequence is serialized per row by this lock, so concurrent
    /// commutative updates never overwrite each other. Returns the computed
    /// payload (also needed for the durable file record).
    ///
    /// The compute closure runs synchronously under the lock and must not
    /// block.
    pub async fn apply_update_to_latest(
        &self,
        timestamp: Timestamp,
        compute: impl FnOnce(&Vec<u8>) -> RS<Vec<u8>>,
    ) -> RS<Vec<u8>> {
        let mut guard = self.inner.lock().await;
        let latest = guard.read_latest()?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                "deferred delta update on a row with no committed version"
            )
        })?;
        let computed = compute(latest.tuple())?;
        guard.write_version_shallow(VersionTuple::new(timestamp, computed.clone()))?;
        Ok(computed)
    }
}

fn build_version_delta(newer: &VersionTuple, older: &VersionTuple) -> VersionDelta {
    VersionDelta::new(
        older.timestamp().clone(),
        older.is_deleted(),
        vec![UpdateDelta::new(
            0,
            newer.tuple().len() as u32,
            older.tuple().clone(),
        )],
    )
}

fn apply_version_delta(version: &mut VersionTuple, delta: &VersionDelta) {
    let mut tuple = version.tuple().clone();
    for item in delta.update_delta() {
        let _ = item.apply_to(&mut tuple);
    }
    *version = if delta.is_deleted() {
        VersionTuple::new_delete(delta.timestamp().clone())
    } else {
        VersionTuple::new(delta.timestamp().clone(), tuple)
    };
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::contract::snapshot::{RunningXList, Snapshot};
    use crate::contract::timestamp::Timestamp;

    fn version(xid: u64, value: &[u8]) -> VersionTuple {
        VersionTuple::new(Timestamp::new(xid, u64::MAX), value.to_vec())
    }

    fn snapshot(xid: u64) -> Snapshot {
        Snapshot::from(RunningXList::new(xid, vec![]))
    }

    #[tokio::test]
    async fn keeps_latest_versions_uncompressed() {
        let row = DataRow::new(1);
        for xid in 1..=6 {
            row.write(version(xid, &[xid as u8]), None).await.unwrap();
        }

        let guard = row.inner.lock().await;
        assert_eq!(guard.tuple.len(), UNCOMPRESSED_VERSION_COUNT);
        assert_eq!(guard.delta.len(), 5);
        assert_eq!(guard.tuple[0].tuple(), &vec![3]);
        assert_eq!(guard.tuple[3].tuple(), &vec![6]);
    }

    #[tokio::test]
    async fn snapshot_preserves_mixed_retained_versions_and_memory_accounting() {
        let row = DataRow::new(1);
        row.write(version(1, &[1; 100]), None).await.unwrap();
        row.write(version(2, &[2; 100]), None).await.unwrap();
        row.write_shallow(version(3, &[3])).await.unwrap();
        let account = Arc::new(AtomicUsize::new(0));
        row.account_memory(account.clone()).await.unwrap();
        assert_eq!(account.load(Ordering::Relaxed), row.resident_bytes().await);
        let restored = DataRow::restore_snapshot(&row.export_snapshot().await.unwrap()).unwrap();
        assert_eq!(
            restored.read(&snapshot(1)).await.unwrap().unwrap().tuple(),
            &vec![1; 100]
        );
        assert_eq!(
            restored.read_latest().await.unwrap().unwrap().tuple(),
            &vec![3]
        );
        row.write_shallow(version(4, &[4; 300])).await.unwrap();
        assert_eq!(account.load(Ordering::Relaxed), row.resident_bytes().await);
        drop(row);
        assert_eq!(account.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn reads_compressed_old_versions_via_delta_chain() {
        let row = DataRow::new(1);
        for xid in 1..=6 {
            row.write(version(xid, &[xid as u8]), None).await.unwrap();
        }

        let visible = row.read(&snapshot(2)).await.unwrap().unwrap();
        assert_eq!(visible.tuple(), &vec![2]);
        assert_eq!(visible.timestamp().c_min(), 2);
    }
}

#[path = "data_row_snapshot.rs"]
mod snapshot_image;

impl Drop for DataRowInner {
    fn drop(&mut self) {
        if let Some(account) = &self.accounting {
            account.fetch_sub(self.bytes, Ordering::Relaxed);
        }
    }
}
