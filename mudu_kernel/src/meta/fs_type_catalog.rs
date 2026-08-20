use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::time::system_time_now;
use std::ops::Bound;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::tuple::build_tuple::build_tuple;
use mudu_type::data_type::DataType;
use mudu_type::data_type_function::send_binary;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;

use crate::contract::fs_type::FsTypeDesc;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::storage::relation::relation::Relation;

/// Partition id shared by all meta catalog relations.
pub const FS_TYPE_CATALOG_PARTITION_ID: OID = 0;
/// Fixed table oid of the filesystem type catalog.
pub const FS_TYPE_CATALOG_TABLE_ID: OID = 0x5;
const FS_TYPE_CATALOG_TABLE_NAME: &str = "__meta_fs_type";
const FS_TYPE_CATALOG_NAME_COLUMN_ID: OID = 0x50001;
const FS_TYPE_CATALOG_ENTRY_COLUMN_ID: OID = 0x50002;

/// Build the schema of the filesystem type catalog table.
pub fn fs_type_catalog_schema() -> SchemaTable {
    SchemaTable::new_with_oid(
        FS_TYPE_CATALOG_TABLE_ID,
        FS_TYPE_CATALOG_TABLE_NAME.to_string(),
        vec![
            SchemaColumn::new_with_oid(
                FS_TYPE_CATALOG_NAME_COLUMN_ID,
                "name".to_string(),
                TypeFamily::String,
                DataType::default_for(TypeFamily::String).to_info(),
            ),
            SchemaColumn::new_with_oid(
                FS_TYPE_CATALOG_ENTRY_COLUMN_ID,
                "entry".to_string(),
                TypeFamily::Binary,
                DataTypeInfo::from_text(TypeFamily::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

/// Build the table descriptor of the filesystem type catalog table.
pub fn fs_type_catalog_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(fs_type_catalog_schema())?.table_desc()
}

/// Open (or create) the filesystem type catalog relation rooted at `path`.
pub async fn open_fs_type_catalog(
    path: &str,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<Relation> {
    let desc = fs_type_catalog_desc()?;
    match async_runtime {
        Some(provider) => {
            Relation::new_with_provider(
                provider,
                FS_TYPE_CATALOG_TABLE_ID,
                FS_TYPE_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
        None => {
            Relation::new(
                FS_TYPE_CATALOG_TABLE_ID,
                FS_TYPE_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
    }
}

/// Encode a filesystem type name into a catalog key tuple.
pub fn encode_fs_type_catalog_key(name: &str) -> RS<Vec<u8>> {
    let desc = fs_type_catalog_desc()?;
    let datum = send_binary(
        &DataValue::from_string(name.to_string()),
        &DataType::default_for(TypeFamily::String),
    )
    .map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Encode,
            "encode fs type catalog key error",
            e
        )
    })?;
    build_tuple(&[datum], desc.key_desc())
}

/// Encode a filesystem type descriptor into a catalog value.
pub fn encode_fs_type_catalog_value(desc: &FsTypeDesc) -> RS<Vec<u8>> {
    rmp_serde::to_vec(desc).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Encode,
            "encode fs type catalog value error",
            e
        )
    })
}

/// Decode a catalog value back into a filesystem type descriptor.
pub fn decode_fs_type_catalog_value(tuple: &[u8]) -> RS<FsTypeDesc> {
    rmp_serde::from_slice(tuple).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "decode fs type catalog value error",
            e
        )
    })
}

/// Replay all filesystem type descriptors stored in the catalog relation.
pub async fn load_fs_types_from_catalog(relation: &Relation) -> RS<Vec<FsTypeDesc>> {
    let rows = relation
        .visible_range(
            (Bound::Unbounded, Bound::Unbounded),
            &WorkerSnapshot::new(visible_snapshot_xid(), vec![]),
        )
        .await?;
    let mut fs_types = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        let fs_type = decode_fs_type_catalog_value(&value)?;
        if encode_fs_type_catalog_key(fs_type.name())? != key {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                format!(
                    "fs type catalog key does not match fs type name {}",
                    fs_type.name()
                )
            ));
        }
        fs_types.push(fs_type);
    }
    Ok(fs_types)
}

fn visible_snapshot_xid() -> u64 {
    let base = system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min((u64::MAX - 2) as u128) as u64;
    base.saturating_add(1)
}

/// Persist a filesystem type descriptor into the catalog relation at `xid`.
pub async fn write_fs_type_to_catalog(relation: &Relation, desc: &FsTypeDesc, xid: u64) -> RS<()> {
    let key = encode_fs_type_catalog_key(desc.name())?;
    let value = encode_fs_type_catalog_value(desc)?;
    relation.write_value(key, value, xid).await?;
    // Catalog relations have no background flush driver: drain the queued
    // PL frames so the DDL is durable when it returns.
    relation.flush_wal_async().await
}

/// Delete a filesystem type entry from the catalog relation at `xid`.
pub async fn delete_fs_type_from_catalog(relation: &Relation, name: &str, xid: u64) -> RS<()> {
    let key = encode_fs_type_catalog_key(name)?;
    relation.write_delete(key, xid).await?;
    relation.flush_wal_async().await
}

/// Return the base directory holding every fs type storage root:
/// `{data_dir}/fs`.
pub fn fs_storage_base(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("fs")
}

/// Return the storage root directory of filesystem `fs_id` under `data_dir`.
pub fn fs_storage_root(data_dir: &str, fs_id: u64) -> PathBuf {
    fs_storage_base(data_dir).join(fs_id.to_string())
}
