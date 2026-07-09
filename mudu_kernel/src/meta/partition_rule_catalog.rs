use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::time::system_time_now;
use std::ops::Bound;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use mudu::common::endian;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;

use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::storage::relation::relation::Relation;

pub const PARTITION_RULE_CATALOG_PARTITION_ID: OID = 0;
pub const PARTITION_RULE_CATALOG_TABLE_ID: OID = 0x2;
const PARTITION_RULE_CATALOG_TABLE_NAME: &str = "__meta_partition_rule";
const PARTITION_RULE_CATALOG_RULE_OID_COLUMN_ID: OID = 0x20001;
const PARTITION_RULE_CATALOG_RULE_COLUMN_ID: OID = 0x20002;

pub fn partition_rule_catalog_schema() -> SchemaTable {
    SchemaTable::new_with_oid(
        PARTITION_RULE_CATALOG_TABLE_ID,
        PARTITION_RULE_CATALOG_TABLE_NAME.to_string(),
        vec![
            SchemaColumn::new_with_oid(
                PARTITION_RULE_CATALOG_RULE_OID_COLUMN_ID,
                "rule_oid".to_string(),
                TypeFamily::U128,
                DataTypeInfo::from_text(TypeFamily::U128, String::new()),
            ),
            SchemaColumn::new_with_oid(
                PARTITION_RULE_CATALOG_RULE_COLUMN_ID,
                "rule".to_string(),
                TypeFamily::Binary,
                DataTypeInfo::from_text(TypeFamily::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

pub fn partition_rule_catalog_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(partition_rule_catalog_schema())?.table_desc()
}

pub async fn open_partition_rule_catalog(
    path: &str,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<Relation> {
    let desc = partition_rule_catalog_desc()?;
    match async_runtime {
        Some(provider) => {
            Relation::new_with_provider(
                provider,
                PARTITION_RULE_CATALOG_TABLE_ID,
                PARTITION_RULE_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
        None => {
            Relation::new(
                PARTITION_RULE_CATALOG_TABLE_ID,
                PARTITION_RULE_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
    }
}

pub fn encode_partition_rule_catalog_key(oid: OID) -> RS<Vec<u8>> {
    let mut key = vec![0; std::mem::size_of::<u128>()];
    endian::write_u128(&mut key, oid);
    Ok(key)
}

pub fn encode_partition_rule_catalog_value(rule: &PartitionRuleDesc) -> RS<Vec<u8>> {
    rmp_serde::to_vec(rule).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Encode,
            "encode partition rule catalog value error",
            e
        )
    })
}

pub fn decode_partition_rule_catalog_key(tuple: &[u8]) -> RS<OID> {
    Ok(endian::read_u128(tuple))
}

pub fn decode_partition_rule_catalog_value(tuple: &[u8]) -> RS<PartitionRuleDesc> {
    rmp_serde::from_slice(tuple).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "decode partition rule catalog value error",
            e
        )
    })
}

pub async fn load_partition_rules_from_catalog(relation: &Relation) -> RS<Vec<PartitionRuleDesc>> {
    let rows = relation
        .visible_range(
            (Bound::Unbounded, Bound::Unbounded),
            &WorkerSnapshot::new(visible_snapshot_xid(), vec![]),
        )
        .await?;
    let mut rules = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        let key_oid = decode_partition_rule_catalog_key(&key)?;
        let rule = decode_partition_rule_catalog_value(&value)?;
        if key_oid != rule.oid {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                format!(
                    "partition rule catalog key oid {} does not match rule oid {}",
                    key_oid, rule.oid
                )
            ));
        }
        rules.push(rule);
    }
    Ok(rules)
}

pub async fn write_partition_rule_to_catalog(
    relation: &Relation,
    rule: &PartitionRuleDesc,
    xid: u64,
) -> RS<()> {
    let key = encode_partition_rule_catalog_key(rule.oid)?;
    let value = encode_partition_rule_catalog_value(rule)?;
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
