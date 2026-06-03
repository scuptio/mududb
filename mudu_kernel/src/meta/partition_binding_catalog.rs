use std::ops::Bound;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mudu::common::endian;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dt_info::DTInfo;

use crate::contract::partition_rule_binding::TablePartitionBinding;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::storage::relation::relation::Relation;

pub const PARTITION_BINDING_CATALOG_PARTITION_ID: OID = 0;
pub const PARTITION_BINDING_CATALOG_TABLE_ID: OID = 0x3;
const PARTITION_BINDING_CATALOG_TABLE_NAME: &str = "__meta_table_partition_binding";
const PARTITION_BINDING_CATALOG_TABLE_OID_COLUMN_ID: OID = 0x30001;
const PARTITION_BINDING_CATALOG_BINDING_COLUMN_ID: OID = 0x30002;

pub fn partition_binding_catalog_schema() -> SchemaTable {
    SchemaTable::new_with_oid(
        PARTITION_BINDING_CATALOG_TABLE_ID,
        PARTITION_BINDING_CATALOG_TABLE_NAME.to_string(),
        vec![
            SchemaColumn::new_with_oid(
                PARTITION_BINDING_CATALOG_TABLE_OID_COLUMN_ID,
                "table_oid".to_string(),
                DatTypeID::U128,
                DTInfo::from_text(DatTypeID::U128, String::new()),
            ),
            SchemaColumn::new_with_oid(
                PARTITION_BINDING_CATALOG_BINDING_COLUMN_ID,
                "binding".to_string(),
                DatTypeID::Binary,
                DTInfo::from_text(DatTypeID::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

pub fn partition_binding_catalog_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(partition_binding_catalog_schema())?.table_desc()
}

pub async fn open_partition_binding_catalog(path: &str) -> RS<Relation> {
    let desc = partition_binding_catalog_desc()?;
    Relation::new(
        PARTITION_BINDING_CATALOG_TABLE_ID,
        PARTITION_BINDING_CATALOG_PARTITION_ID,
        path.to_string(),
        desc.as_ref(),
    )
    .await
}

pub fn encode_partition_binding_catalog_key(oid: OID) -> RS<Vec<u8>> {
    let mut key = vec![0; std::mem::size_of::<u128>()];
    endian::write_u128(&mut key, oid);
    Ok(key)
}

pub fn encode_partition_binding_catalog_value(binding: &TablePartitionBinding) -> RS<Vec<u8>> {
    rmp_serde::to_vec(binding).map_err(|e| {
        mudu::m_error!(
            mudu::error::ec::EC::EncodeErr,
            "encode partition binding catalog value error",
            e
        )
    })
}

pub fn decode_partition_binding_catalog_key(tuple: &[u8]) -> RS<OID> {
    Ok(endian::read_u128(tuple))
}

pub fn decode_partition_binding_catalog_value(tuple: &[u8]) -> RS<TablePartitionBinding> {
    rmp_serde::from_slice(tuple).map_err(|e| {
        mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "decode partition binding catalog value error",
            e
        )
    })
}

pub async fn load_partition_bindings_from_catalog(relation: &Relation) -> RS<Vec<TablePartitionBinding>> {
    let rows = relation.visible_range(
        (Bound::Unbounded, Bound::Unbounded),
        &WorkerSnapshot::new(visible_snapshot_xid(), vec![]),
    ).await?;
    let mut bindings = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        let key_oid = decode_partition_binding_catalog_key(&key)?;
        let binding = decode_partition_binding_catalog_value(&value)?;
        if key_oid != binding.table_id {
            return Err(mudu::m_error!(
                mudu::error::ec::EC::DecodeErr,
                format!(
                    "partition binding catalog key oid {} does not match table oid {}",
                    key_oid, binding.table_id
                )
            ));
        }
        bindings.push(binding);
    }
    Ok(bindings)
}

pub async fn write_partition_binding_to_catalog(
    relation: &Relation,
    binding: &TablePartitionBinding,
    xid: u64,
) -> RS<()> {
    let key = encode_partition_binding_catalog_key(binding.table_id)?;
    let value = encode_partition_binding_catalog_value(binding)?;
    relation.write_value(key, value, xid).await
}

fn visible_snapshot_xid() -> u64 {
    let base = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min((u64::MAX - 2) as u128) as u64;
    base.saturating_add(1)
}
