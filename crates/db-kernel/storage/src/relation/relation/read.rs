use super::*;
use std::ops::Bound;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::time_series::time_series_file::TimeSeriesFile;
use ::catalog::contract::data_row::DataRow;
use ::catalog::contract::snapshot::Snapshot;
use ::catalog::contract::version_tuple::VersionTuple;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::index::index_key::key_tuple::KeyTuple;
use mudu_utils::scoped_task_trace;

impl RelationInner {
    pub(crate) async fn visible_meta(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<(OID, VersionTuple, bool)>> {
        scoped_task_trace!();
        let row = {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::VisIndexGet);
            match self.index.get(key)? {
                Some(row) => row,
                None => return Ok(None),
            }
        };
        let tuple_id = row
            .tuple_id()
            .await?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
        let snapshot = snapshot.to_snapshot();
        let visible = {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::VisVersionRead,
            );
            read_visible_version_async(&row, &snapshot).await
        };
        Ok(visible
            .filter(|(version, _)| !version.is_deleted())
            .map(|(version, payload_authoritative)| (tuple_id, version, payload_authoritative)))
    }

    pub(crate) async fn visible_value(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        scoped_task_trace!();
        let Some((tuple_id, version, payload_authoritative)) =
            self.visible_meta(key, snapshot).await?
        else {
            return Ok(None);
        };
        // Hot path: the visible version still lives in the retained
        // in-memory window and carries its own payload, so the value file's
        // page chain is not touched at all. Versions rebuilt from files or
        // reconstructed through the metadata-only delta chain fall back to
        // the value file, which remains the durable source of the bytes.
        if payload_authoritative {
            return Ok(Some(version.tuple_into()));
        }
        let payload = {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::VisPageRead);
            self.read_value_payload(version.timestamp().c_min(), tuple_id)
                .await?
        };
        Ok(Some(payload))
    }

    pub(crate) async fn visible_range(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let begin_key = bounds.0.as_ref().map(|key| KeyTuple::from(key.to_vec()));
        let end_key = bounds.1.as_ref().map(|key| KeyTuple::from(key.to_vec()));
        let rows = self
            .index
            .range((bound_key_ref(&begin_key), bound_key_ref(&end_key)))?;

        let snapshot = snapshot.to_snapshot();
        let mut items = Vec::new();
        for (key, row) in rows {
            // The index key already holds the key bytes; no need to walk the
            // key file's version chain again for every row.
            if let Some(pair) =
                visible_payloads_async(&self.value_file, key.as_slice().to_vec(), &row, &snapshot)
                    .await?
            {
                items.push(pair);
            }
        }
        Ok(items)
    }

    pub(crate) async fn has_write_conflict(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<bool> {
        let latest = match self.index.get(key)? {
            Some(row) => latest_version_async(&row).await,
            None => None,
        };
        Ok(latest
            .map(|latest| !snapshot.is_visible(latest.timestamp().c_min()))
            .unwrap_or(false))
    }

    async fn read_value_payload(&self, timestamp: u64, tuple_id: OID) -> RS<Vec<u8>> {
        let record = self.value_file.get(timestamp, tuple_id as u64).await?;
        record.map(|record| record.payload).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("missing value payload ts={timestamp} tuple_id={tuple_id}")
            )
        })
    }
}

async fn visible_payloads_async(
    value_file: &TimeSeriesFile,
    key: Vec<u8>,
    row: &DataRow,
    snapshot: &Snapshot,
) -> RS<Option<(Vec<u8>, Vec<u8>)>> {
    let tuple_id = row
        .tuple_id()
        .await?
        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
    let Some((version, payload_authoritative)) = read_visible_version_async(row, snapshot)
        .await
        .filter(|(version, _)| !version.is_deleted())
    else {
        return Ok(None);
    };
    // Same split as visible_value: in-memory payload when the retained
    // window carries it, value-file page chain otherwise.
    if payload_authoritative {
        return Ok(Some((key, version.tuple_into())));
    }
    let ts = version.timestamp().c_min();
    let value = value_file
        .get(ts, tuple_id as u64)
        .await?
        .map(|record| record.payload)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("missing value payload ts={ts} tuple_id={tuple_id}")
            )
        })?;
    Ok(Some((key, value)))
}

async fn latest_version_async(row: &DataRow) -> Option<VersionTuple> {
    row.read_latest().await.ok().flatten()
}

async fn read_visible_version_async(
    row: &DataRow,
    snapshot: &Snapshot,
) -> Option<(VersionTuple, bool)> {
    row.read_detailed(snapshot).await.ok().flatten()
}

fn bound_key_ref(bound: &Bound<KeyTuple>) -> Bound<&KeyTuple> {
    match bound {
        Bound::Included(key) => Bound::Included(key),
        Bound::Excluded(key) => Bound::Excluded(key),
        Bound::Unbounded => Bound::Unbounded,
    }
}
