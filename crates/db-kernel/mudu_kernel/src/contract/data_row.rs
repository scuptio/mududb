use crate::contract::snapshot::Snapshot;
use crate::contract::timestamp::Timestamp;
use crate::contract::version_delta::VersionDelta;
use crate::contract::version_tuple::VersionTuple;
use mudu::common::id::{TupleID, OID};
use mudu::common::result::RS;
use mudu::common::update_delta::UpdateDelta;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_utils::scoped_task_trace;
use std::sync::Arc;

const UNCOMPRESSED_VERSION_COUNT: usize = 4;

#[derive(Clone)]
pub struct DataRow {
    inner: Arc<SMutex<DataRowInner>>,
}

struct DataRowInner {
    tid: TupleID,
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
            self.delta.push(delta);
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
            self.delta.push(delta);
        }
        self.push_version(version);
        Ok(())
    }

    fn push_version(&mut self, version: VersionTuple) {
        self.tuple.push(version);
        if self.tuple.len() > UNCOMPRESSED_VERSION_COUNT {
            self.tuple.remove(0);
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
    pub fn new(tid: TupleID) -> Self {
        Self {
            inner: Arc::new(SMutex::new(DataRowInner::new(tid))),
        }
    }

    pub async fn tuple_id(&self) -> RS<Option<OID>> {
        mudu_utils::scoped_task_trace!();
        self.tuple_id_sync()
    }

    pub fn tuple_id_sync(&self) -> RS<Option<OID>> {
        let guard = self.inner.lock()?;
        Ok(Some(guard.tid as OID))
    }

    pub async fn read(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        self.read_sync(snapshot)
    }

    pub fn read_sync(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        let guard = self.inner.lock()?;
        guard.read_version(snapshot)
    }

    /// Reads the visible version and whether its payload bytes are the
    /// version's own; see [`DataRowInner::read_version_detailed`].
    pub async fn read_detailed(&self, snapshot: &Snapshot) -> RS<Option<(VersionTuple, bool)>> {
        self.read_detailed_sync(snapshot)
    }

    /// Synchronous variant of [`DataRow::read_detailed`].
    pub fn read_detailed_sync(&self, snapshot: &Snapshot) -> RS<Option<(VersionTuple, bool)>> {
        let guard = self.inner.lock()?;
        guard.read_version_detailed(snapshot)
    }

    pub async fn read_latest(&self) -> RS<Option<VersionTuple>> {
        self.read_latest_sync()
    }

    pub fn read_latest_sync(&self) -> RS<Option<VersionTuple>> {
        let guard = self.inner.lock()?;
        guard.read_latest()
    }

    pub async fn write(&self, version: VersionTuple, prev_version: Option<VersionDelta>) -> RS<()> {
        scoped_task_trace!();
        self.write_sync(version, prev_version)
    }

    pub fn write_sync(&self, version: VersionTuple, prev_version: Option<VersionDelta>) -> RS<()> {
        scoped_task_trace!();
        let mut guard = self.inner.lock()?;
        guard.write_version(version, prev_version)
    }

    /// Appends a version whose payload is kept only in the retained
    /// full-version window; the delta chain records only metadata (timestamp
    /// and deleted flag), never payload bytes. See
    /// [`DataRowInner::write_version_shallow`].
    pub async fn write_shallow(&self, version: VersionTuple) -> RS<()> {
        scoped_task_trace!();
        self.write_shallow_sync(version)
    }

    /// Synchronous variant of [`DataRow::write_shallow`].
    pub fn write_shallow_sync(&self, version: VersionTuple) -> RS<()> {
        scoped_task_trace!();
        let mut guard = self.inner.lock()?;
        guard.write_version_shallow(version)
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
    pub fn apply_update_to_latest_sync(
        &self,
        timestamp: Timestamp,
        compute: impl FnOnce(&Vec<u8>) -> RS<Vec<u8>>,
    ) -> RS<Vec<u8>> {
        let mut guard = self.inner.lock()?;
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

unsafe impl Send for DataRow {}
unsafe impl Sync for DataRow {}

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

    #[test]
    fn keeps_latest_versions_uncompressed() {
        let row = DataRow::new(1);
        for xid in 1..=6 {
            row.write_sync(version(xid, &[xid as u8]), None).unwrap();
        }

        let guard = row.inner.lock().unwrap();
        assert_eq!(guard.tuple.len(), UNCOMPRESSED_VERSION_COUNT);
        assert_eq!(guard.delta.len(), 5);
        assert_eq!(guard.tuple[0].tuple(), &vec![3]);
        assert_eq!(guard.tuple[3].tuple(), &vec![6]);
    }

    #[test]
    fn reads_compressed_old_versions_via_delta_chain() {
        let row = DataRow::new(1);
        for xid in 1..=6 {
            row.write_sync(version(xid, &[xid as u8]), None).unwrap();
        }

        let visible = row.read_sync(&snapshot(2)).unwrap().unwrap();
        assert_eq!(visible.tuple(), &vec![2]);
        assert_eq!(visible.timestamp().c_min(), 2);
    }
}
