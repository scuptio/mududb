//! Offline schema inspection used before opening mutable catalog owners.
use super::*;
use std::path::Path;

/// Reads physically persisted user schemas without creating files, replaying
/// WAL or publishing an index. The caller must own a stopped, flushed catalog.
/// `data_dir` is the database root (the catalog lives in its `meta` child).
/// None means both relation components are absent; a partial pair is an error.
/// This is an inspection primitive, not proof that the catalog matches Raft.
pub async fn read_persisted_schemas(
    data_dir: &Path,
    max_bytes: u64,
) -> RS<Option<Vec<SchemaClass>>> {
    let path = data_dir.join("meta");
    let fs = mudu_sys::fs::tokio_fs();
    let component = |index| {
        storage::time_series::time_series_file::TimeSeriesFile::relation_file_path(
            &path,
            SCHEMA_CATALOG_PARTITION_ID,
            SCHEMA_CATALOG_TABLE_ID,
            index,
        )
    };
    let key_exists = fs.path_exists(&component(0)).await?;
    let value_exists = fs.path_exists(&component(1)).await?;
    if !key_exists && !value_exists {
        return Ok(None);
    }
    let desc = schema_catalog_desc()?;
    let rows = storage::relation::relation::read_persisted_relation_latest(
        fs.as_ref(),
        &path,
        SCHEMA_CATALOG_TABLE_ID,
        SCHEMA_CATALOG_PARTITION_ID,
        desc.as_ref(),
        max_bytes,
    )
    .await?;
    let mut schemas = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        if key.len() != std::mem::size_of::<OID>() {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::CorruptedData,
                "invalid persisted schema key length"
            ));
        }
        let schema = decode_schema_catalog_value(&value)?;
        if decode_schema_catalog_key(&key)? != schema.id() {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::CorruptedData,
                "persisted schema key and object identity disagree"
            ));
        }
        schemas.push(schema);
    }
    Ok(Some(schemas))
}

#[cfg(test)]
mod tests;
