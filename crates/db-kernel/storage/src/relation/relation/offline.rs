//! Inspect a flushed relation without WAL replay or index reconstruction.
use super::{open::tuple_schema_hash, KEY_FILE_INDEX, VALUE_FILE_INDEX};
use crate::time_series::time_series_file::{read_persisted_records, TimeSeriesFile};
use catalog::contract::table_desc::TableDesc;
use mudu::{
    common::{id::OID, result::RS},
    error::ErrorCode,
    mudu_error,
};
use mudu_sys::contract::async_fs::AsyncFs;
use std::{collections::BTreeMap, path::Path};

/// Reads the latest physically persisted rows of an exclusively owned, offline
/// relation. The caller must establish a flushed cut and prevent concurrent
/// writes. This does not replay WAL, create files, rebuild indexes or recover
/// unflushed changes. A missing value at the latest key version is a tombstone,
/// matching normal relation reconstruction; history must prove expected rows.
/// `max_bytes` bounds the combined source component sizes; each component
/// is checked before its pages are scanned.
/// Results are ordered by raw encoded key bytes, not SQL collation.
pub async fn read_persisted_relation_latest(
    fs: &dyn AsyncFs,
    path: &Path,
    table: OID,
    partition: OID,
    descriptor: &TableDesc,
    max_bytes: u64,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    if descriptor.id() != table {
        return Err(invalid("offline relation descriptor identity mismatch"));
    }
    let key_path = TimeSeriesFile::relation_file_path(path, partition, table, KEY_FILE_INDEX);
    let value_path = TimeSeriesFile::relation_file_path(path, partition, table, VALUE_FILE_INDEX);
    let mut budget = max_bytes;
    let keys = read_persisted_records(
        fs,
        &key_path,
        tuple_schema_hash(b'K', descriptor.key_desc()),
        &mut budget,
    )
    .await?;
    let values = read_persisted_records(
        fs,
        &value_path,
        tuple_schema_hash(b'V', descriptor.value_desc()),
        &mut budget,
    )
    .await?;
    let mut values: BTreeMap<_, _> = values
        .into_iter()
        .map(|v| ((v.timestamp, v.tuple_id), v.payload))
        .collect();
    let mut latest = BTreeMap::<Vec<u8>, (u64, u64)>::new();
    let mut identities = BTreeMap::new();
    for key in keys {
        if let Some(previous) = identities.insert(key.tuple_id, key.payload.clone()) {
            if previous != key.payload {
                return Err(invalid("relation tuple id belongs to multiple keys"));
            }
        }
        if let Some((timestamp, tuple)) = latest.get_mut(&key.payload) {
            if *tuple != key.tuple_id {
                return Err(invalid("relation key belongs to multiple tuple ids"));
            }
            *timestamp = (*timestamp).max(key.timestamp);
        } else {
            latest.insert(key.payload, (key.timestamp, key.tuple_id));
        }
    }
    Ok(latest
        .into_iter()
        .filter_map(|(key, version)| values.remove(&version).map(|value| (key, value)))
        .collect())
}
fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}

#[cfg(test)]
mod tests;
