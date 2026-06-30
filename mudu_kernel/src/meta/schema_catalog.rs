use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::time::system_time_now;
use std::ops::Bound;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use mudu::common::endian;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dt_info::DTInfo;

use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::storage::relation::relation::Relation;

pub const SCHEMA_CATALOG_PARTITION_ID: OID = 0;
pub const SCHEMA_CATALOG_TABLE_ID: OID = 0x1;
const SCHEMA_CATALOG_TABLE_NAME: &str = "__meta_schema_table";
const SCHEMA_CATALOG_TABLE_OID_COLUMN_ID: OID = 0x10001;
const SCHEMA_CATALOG_SCHEMA_COLUMN_ID: OID = 0x10002;

pub fn schema_catalog_schema() -> SchemaTable {
    SchemaTable::new_with_oid(
        SCHEMA_CATALOG_TABLE_ID,
        SCHEMA_CATALOG_TABLE_NAME.to_string(),
        vec![
            SchemaColumn::new_with_oid(
                SCHEMA_CATALOG_TABLE_OID_COLUMN_ID,
                "table_oid".to_string(),
                DatTypeID::U128,
                DTInfo::from_text(DatTypeID::U128, String::new()),
            ),
            SchemaColumn::new_with_oid(
                SCHEMA_CATALOG_SCHEMA_COLUMN_ID,
                "schema".to_string(),
                DatTypeID::Binary,
                DTInfo::from_text(DatTypeID::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

pub fn schema_catalog_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(schema_catalog_schema())?.table_desc()
}

pub async fn open_schema_catalog(
    path: &str,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<Relation> {
    let desc = schema_catalog_desc()?;
    match async_runtime {
        Some(provider) => {
            Relation::new_with_provider(
                provider,
                SCHEMA_CATALOG_TABLE_ID,
                SCHEMA_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
        None => {
            Relation::new(
                SCHEMA_CATALOG_TABLE_ID,
                SCHEMA_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
    }
}

pub fn encode_schema_catalog_key(oid: OID) -> RS<Vec<u8>> {
    let mut key = vec![0; std::mem::size_of::<u128>()];
    endian::write_u128(&mut key, oid);
    Ok(key)
}

pub fn encode_schema_catalog_value(schema: &SchemaTable) -> RS<Vec<u8>> {
    rmp_serde::to_vec(schema).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Encode,
            "encode schema catalog schema error",
            e
        )
    })
}

pub fn decode_schema_catalog_key(tuple: &[u8]) -> RS<OID> {
    Ok(endian::read_u128(tuple))
}

pub fn decode_schema_catalog_value(tuple: &[u8]) -> RS<SchemaTable> {
    rmp_serde::from_slice(tuple).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "decode schema catalog schema error",
            e
        )
    })
}

pub async fn load_schemas_from_catalog(relation: &Relation) -> RS<Vec<SchemaTable>> {
    let rows = relation
        .visible_range(
            (Bound::Unbounded, Bound::Unbounded),
            &WorkerSnapshot::new(visible_snapshot_xid(), vec![]),
        )
        .await?;
    let mut schemas = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        let key_oid = decode_schema_catalog_key(&key)?;
        let schema = decode_schema_catalog_value(&value)?;
        if key_oid != schema.id() {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                format!(
                    "schema catalog key oid {} does not match schema oid {}",
                    key_oid,
                    schema.id()
                )
            ));
        }
        schemas.push(schema);
    }
    Ok(schemas)
}

fn visible_snapshot_xid() -> u64 {
    let base = system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min((u64::MAX - 2) as u128) as u64;
    base.saturating_add(1)
}

pub async fn write_schema_to_catalog(
    relation: &Relation,
    schema: &SchemaTable,
    xid: u64,
) -> RS<()> {
    let key = encode_schema_catalog_key(schema.id())?;
    let value = encode_schema_catalog_value(schema)?;
    relation.write_value(key, value, xid).await
}

pub async fn delete_schema_from_catalog(relation: &Relation, oid: OID, xid: u64) -> RS<()> {
    let key = encode_schema_catalog_key(oid)?;
    relation.write_delete(key, xid).await
}
