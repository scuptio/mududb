use crate::contract::snapshot::Snapshot;
use crate::contract::version_delta::VersionDelta;
use crate::contract::version_tuple::VersionTuple;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::update_delta::UpdateDelta;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_TUPLE_VERSION_SIZE: usize = 4;

#[derive(Clone)]
pub struct DataRow {
    inner: Arc<Mutex<DataRowInner>>,
}

struct DataRowInner {
    tuple: Vec<(VersionTuple, Vec<UpdateDelta>)>,
    delta: Vec<VersionDelta>,
}

impl DataRowInner {
    fn new() -> Self {
        Self {
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
        if self.tuple.is_empty() {
            self.tuple.push((version, vec![]));
        } else {
            {
                let prev_version = prev_version.unwrap();
                let last = self.tuple.last_mut().unwrap();
                if !last.0.timestamp().eq(prev_version.timestamp()) {
                    panic!("error, timestamp")
                }
                last.1 = prev_version.update_delta_into();
            }
            if self.tuple.len() < MAX_TUPLE_VERSION_SIZE {
                self.tuple.push((version, vec![]));
            } else {
                let mut tuple: Vec<_> = self.tuple.drain(1..).collect();
                std::mem::swap(&mut self.tuple, &mut tuple);
                let (v_t, v_d) = tuple.pop().unwrap();
                let v_delta = VersionDelta::new(v_t.timestamp_into(), v_d);
                self.delta.push(v_delta);
            }
        }

        Ok(())
    }

    fn read_latest(&self) -> RS<Option<VersionTuple>> {
        Ok(self.tuple.last().map(|e| e.0.clone()))
    }

    fn read_version(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        let r = self.read_version_in_tuple_array(snapshot)?;
        match r {
            Ok(v) => Ok(Some(v)),
            Err(opt) => match opt {
                Some(v) => {
                    let opt_v = self.read_version_in_delta_list(snapshot, v)?;
                    Ok(opt_v)
                }
                None => Ok(None),
            },
        }
    }

    fn read_version_in_tuple_array(
        &self,
        snapshot: &Snapshot,
    ) -> RS<Result<VersionTuple, Option<VersionTuple>>> {
        let mut opt_last: Option<&VersionTuple> = None;
        for (v, _d) in self.tuple.iter().rev() {
            let ts = v.timestamp();
            if snapshot.is_tuple_visible(ts) {
                return Ok(Ok(v.clone()));
            } else {
                opt_last = Some(v);
            }
        }
        let ret = opt_last.cloned();
        Ok(Err(ret))
    }

    fn read_version_in_delta_list(
        &self,
        snapshot: &Snapshot,
        prev_version: VersionTuple,
    ) -> RS<Option<VersionTuple>> {
        let mut version = prev_version;
        let mut vec_delta = vec![];
        let mut opt_ts = None;
        for version_delta in self.delta.iter().rev() {
            let ts = version_delta.timestamp();
            for delta in version_delta.update_delta().iter() {
                vec_delta.push(delta);
            }
            if snapshot.is_tuple_visible(ts) {
                opt_ts = Some(ts.clone());
                break;
            }
        }

        if let Some(ts) = opt_ts {
            assert!(!vec_delta.is_empty());
            for delta in vec_delta {
                let tuple = version.mut_tuple();
                let _ = delta.apply_to(tuple);
            }
            version.update_timestamp(ts);
            Ok(Some(version))
        } else {
            Ok(None)
        }
    }
}

impl DataRow {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(DataRowInner::new())),
        }
    }

    pub async fn tuple_id(&self) -> RS<Option<OID>> {
        todo!()
    }

    pub async fn read(&self, snapshot: &Snapshot) -> RS<Option<VersionTuple>> {
        let guard = self.inner.lock().await;
        guard.read_version(snapshot)
    }

    pub async fn read_latest(&self) -> RS<Option<VersionTuple>> {
        let guard = self.inner.lock().await;
        guard.read_latest()
    }

    pub async fn write(&self, version: VersionTuple, prev_version: Option<VersionDelta>) -> RS<()> {
        let mut guard = self.inner.lock().await;
        guard.write_version(version, prev_version)
    }
}
