use crate::contract::snapshot::Snapshot;
use crate::contract::version_delta::VersionDelta;
use crate::contract::version_tuple::VersionTuple;
use mudu::common::id::{TupleID, OID};
use mudu::common::result::RS;
use mudu::common::update_delta::UpdateDelta;
use std::sync::{Arc, Mutex};

const UNCOMPRESSED_VERSION_COUNT: usize = 4;

#[derive(Clone)]
pub struct DataRow {
    inner: Arc<Mutex<DataRowInner>>,
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
        self.tuple.push(version);
        if self.tuple.len() > UNCOMPRESSED_VERSION_COUNT {
            self.tuple.remove(0);
        }
        Ok(())
    }

    fn read_latest(&self) -> RS<Option<VersionTuple>> {
        Ok(self.tuple.last().cloned())
    }

    fn read_version(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        if let Some(version) = self
            .tuple
            .iter()
            .rev()
            .find(|v| snapshot.is_tuple_visible(v.timestamp()))
            .cloned()
        {
            return Ok(Some(version));
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
                return Ok(Some(version));
            }
        }

        Ok(None)
    }
}

impl DataRow {
    pub fn new(tid: TupleID) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DataRowInner::new(tid))),
        }
    }

    pub async fn tuple_id(&self) -> RS<Option<OID>> {
        mudu_utils::scoped_task_trace!();
        self.tuple_id_sync()
    }

    pub fn tuple_id_sync(&self) -> RS<Option<OID>> {
        let guard = self.inner.lock().unwrap();
        Ok(Some(guard.tid as OID))
    }

    pub async fn read(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        self.read_sync(snapshot)
    }

    pub fn read_sync(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        let guard = self.inner.lock().unwrap();
        guard.read_version(snapshot)
    }

    pub async fn read_latest(&self) -> RS<Option<VersionTuple>> {
        self.read_latest_sync()
    }

    pub fn read_latest_sync(&self) -> RS<Option<VersionTuple>> {
        let guard = self.inner.lock().unwrap();
        guard.read_latest()
    }

    pub async fn write(&self, version: VersionTuple, prev_version: Option<VersionDelta>) -> RS<()> {
        self.write_sync(version, prev_version)
    }

    pub fn write_sync(&self, version: VersionTuple, prev_version: Option<VersionDelta>) -> RS<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.write_version(version, prev_version)
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
