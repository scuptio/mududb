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

use crate::contract::partition_rule_binding::PartitionPlacement;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::storage::relation::relation::Relation;

pub const PARTITION_PLACEMENT_CATALOG_PARTITION_ID: OID = 0;
pub const PARTITION_PLACEMENT_CATALOG_TABLE_ID: OID = 0x4;
const PARTITION_PLACEMENT_CATALOG_TABLE_NAME: &str = "__meta_partition_placement";
const PARTITION_PLACEMENT_CATALOG_PARTITION_OID_COLUMN_ID: OID = 0x40001;
const PARTITION_PLACEMENT_CATALOG_PLACEMENT_COLUMN_ID: OID = 0x40002;

pub fn partition_placement_catalog_schema() -> SchemaTable {
    SchemaTable::new_with_oid(
        PARTITION_PLACEMENT_CATALOG_TABLE_ID,
        PARTITION_PLACEMENT_CATALOG_TABLE_NAME.to_string(),
        vec![
            SchemaColumn::new_with_oid(
                PARTITION_PLACEMENT_CATALOG_PARTITION_OID_COLUMN_ID,
                "partition_oid".to_string(),
                DatTypeID::U128,
                DTInfo::from_text(DatTypeID::U128, String::new()),
            ),
            SchemaColumn::new_with_oid(
                PARTITION_PLACEMENT_CATALOG_PLACEMENT_COLUMN_ID,
                "placement".to_string(),
                DatTypeID::Binary,
                DTInfo::from_text(DatTypeID::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

pub fn partition_placement_catalog_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(partition_placement_catalog_schema())?.table_desc()
}

pub async fn open_partition_placement_catalog(
    path: &str,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<Relation> {
    let desc = partition_placement_catalog_desc()?;
    match async_runtime {
        Some(provider) => {
            Relation::new_with_provider(
                provider,
                PARTITION_PLACEMENT_CATALOG_TABLE_ID,
                PARTITION_PLACEMENT_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
        None => {
            Relation::new(
                PARTITION_PLACEMENT_CATALOG_TABLE_ID,
                PARTITION_PLACEMENT_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
    }
}

pub fn encode_partition_placement_catalog_key(oid: OID) -> RS<Vec<u8>> {
    let mut key = vec![0; std::mem::size_of::<u128>()];
    endian::write_u128(&mut key, oid);
    Ok(key)
}

pub fn encode_partition_placement_catalog_value(placement: &PartitionPlacement) -> RS<Vec<u8>> {
    rmp_serde::to_vec(placement).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Encode,
            "encode partition placement catalog value error",
            e
        )
    })
}

pub fn decode_partition_placement_catalog_key(tuple: &[u8]) -> RS<OID> {
    Ok(endian::read_u128(tuple))
}

pub fn decode_partition_placement_catalog_value(tuple: &[u8]) -> RS<PartitionPlacement> {
    rmp_serde::from_slice(tuple).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "decode partition placement catalog value error",
            e
        )
    })
}

pub async fn load_partition_placements_from_catalog(
    relation: &Relation,
) -> RS<Vec<PartitionPlacement>> {
    let rows = relation
        .visible_range(
            (Bound::Unbounded, Bound::Unbounded),
            &WorkerSnapshot::new(visible_snapshot_xid(), vec![]),
        )
        .await?;
    let mut placements = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        let key_oid = decode_partition_placement_catalog_key(&key)?;
        let placement = decode_partition_placement_catalog_value(&value)?;
        if key_oid != placement.partition_id {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                format!(
                    "partition placement catalog key oid {} does not match partition oid {}",
                    key_oid, placement.partition_id
                )
            ));
        }
        placements.push(placement);
    }
    Ok(placements)
}

pub async fn write_partition_placement_to_catalog(
    relation: &Relation,
    placement: &PartitionPlacement,
    xid: u64,
) -> RS<()> {
    let key = encode_partition_placement_catalog_key(placement.partition_id)?;
    let value = encode_partition_placement_catalog_value(placement)?;
    relation.write_value(key, value, xid).await
}

fn visible_snapshot_xid() -> u64 {
    let base = system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min((u64::MAX - 2) as u128) as u64;
    base.saturating_add(1)
}
