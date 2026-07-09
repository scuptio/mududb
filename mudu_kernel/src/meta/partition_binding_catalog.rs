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
                TypeFamily::U128,
                DataTypeInfo::from_text(TypeFamily::U128, String::new()),
            ),
            SchemaColumn::new_with_oid(
                PARTITION_BINDING_CATALOG_BINDING_COLUMN_ID,
                "binding".to_string(),
                TypeFamily::Binary,
                DataTypeInfo::from_text(TypeFamily::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

pub fn partition_binding_catalog_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(partition_binding_catalog_schema())?.table_desc()
}

pub async fn open_partition_binding_catalog(
    path: &str,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<Relation> {
    let desc = partition_binding_catalog_desc()?;
    match async_runtime {
        Some(provider) => {
            Relation::new_with_provider(
                provider,
                PARTITION_BINDING_CATALOG_TABLE_ID,
                PARTITION_BINDING_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
        None => {
            Relation::new(
                PARTITION_BINDING_CATALOG_TABLE_ID,
                PARTITION_BINDING_CATALOG_PARTITION_ID,
                path.to_string(),
                desc.as_ref(),
            )
            .await
        }
    }
}

pub fn encode_partition_binding_catalog_key(oid: OID) -> RS<Vec<u8>> {
    let mut key = vec![0; std::mem::size_of::<u128>()];
    endian::write_u128(&mut key, oid);
    Ok(key)
}

pub fn encode_partition_binding_catalog_value(binding: &TablePartitionBinding) -> RS<Vec<u8>> {
    rmp_serde::to_vec(binding).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Encode,
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
        mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "decode partition binding catalog value error",
            e
        )
    })
}

pub async fn load_partition_bindings_from_catalog(
    relation: &Relation,
) -> RS<Vec<TablePartitionBinding>> {
    let rows = relation
        .visible_range(
            (Bound::Unbounded, Bound::Unbounded),
            &WorkerSnapshot::new(visible_snapshot_xid(), vec![]),
        )
        .await?;
    let mut bindings = Vec::with_capacity(rows.len());
    for (key, value) in rows {
        let key_oid = decode_partition_binding_catalog_key(&key)?;
        let binding = decode_partition_binding_catalog_value(&value)?;
        if key_oid != binding.table_id {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
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
    let base = system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min((u64::MAX - 2) as u128) as u64;
    base.saturating_add(1)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use mudu::error::ErrorCode;
    use mudu_sys::env_var::temp_dir;
    use mudu_type::type_family::TypeFamily;

    use crate::contract::partition_rule_binding::TablePartitionBinding;
    use crate::meta::partition_binding_catalog::{
        decode_partition_binding_catalog_key, decode_partition_binding_catalog_value,
        encode_partition_binding_catalog_key, encode_partition_binding_catalog_value,
        load_partition_bindings_from_catalog, open_partition_binding_catalog,
        partition_binding_catalog_desc, partition_binding_catalog_schema,
        write_partition_binding_to_catalog, PARTITION_BINDING_CATALOG_PARTITION_ID,
        PARTITION_BINDING_CATALOG_TABLE_ID, PARTITION_BINDING_CATALOG_TABLE_NAME,
    };

    fn catalog_path(name: &str) -> String {
        temp_dir()
            .join(format!(
                "partition_binding_catalog_{}_{}",
                name,
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn schema_has_expected_columns() {
        let schema = partition_binding_catalog_schema();
        assert_eq!(schema.id(), PARTITION_BINDING_CATALOG_TABLE_ID);
        assert_eq!(schema.id(), 0x3);

        let columns = schema.columns();
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].get_oid(), 0x30001);
        assert_eq!(columns[0].get_name(), "table_oid");
        assert_eq!(columns[0].type_id(), TypeFamily::U128);
        assert_eq!(columns[1].get_oid(), 0x30002);
        assert_eq!(columns[1].get_name(), "binding");
        assert_eq!(columns[1].type_id(), TypeFamily::Binary);

        assert_eq!(schema.key_indices(), &vec![0]);
        assert_eq!(schema.value_indices(), &vec![1]);
    }

    #[test]
    fn partition_binding_catalog_desc_succeeds() {
        let desc = partition_binding_catalog_desc().unwrap();
        assert_eq!(desc.id(), PARTITION_BINDING_CATALOG_TABLE_ID);
        assert_eq!(desc.name(), PARTITION_BINDING_CATALOG_TABLE_NAME);
        assert_eq!(desc.key_indices(), &vec![0]);
        assert_eq!(desc.value_indices(), &vec![1]);
    }

    #[test]
    fn encode_decode_key_roundtrip() {
        let oid = 0x1234_5678_90AB_CDEF_1234_5678_90AB_CDEF_u128;
        let key = encode_partition_binding_catalog_key(oid).unwrap();
        let decoded = decode_partition_binding_catalog_key(&key).unwrap();
        assert_eq!(decoded, oid);
    }

    #[test]
    fn encode_decode_value_roundtrip() {
        let binding = TablePartitionBinding {
            table_id: 0x10,
            rule_id: 0x20,
            ref_attr_indices: vec![0, 2, 4],
        };
        let value = encode_partition_binding_catalog_value(&binding).unwrap();
        let decoded = decode_partition_binding_catalog_value(&value).unwrap();
        assert_eq!(decoded, binding);
    }

    #[test]
    fn decode_value_rejects_invalid_msgpack() {
        let invalid = vec![0xFF];
        let err = decode_partition_binding_catalog_value(&invalid).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn load_bindings_rejects_key_value_table_id_mismatch() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let path = catalog_path("mismatch");
            let relation = open_partition_binding_catalog(&path, None).await.unwrap();

            let key = encode_partition_binding_catalog_key(0x10).unwrap();
            let binding = TablePartitionBinding {
                table_id: 0x20,
                rule_id: 0,
                ref_attr_indices: vec![],
            };
            let value = encode_partition_binding_catalog_value(&binding).unwrap();
            relation.write_value(key, value, 1).await.unwrap();

            let err = load_partition_bindings_from_catalog(&relation)
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Decode);
        })
        .unwrap();
    }

    #[test]
    fn write_and_load_binding_roundtrip() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let path = catalog_path("roundtrip");
            let relation = open_partition_binding_catalog(&path, None).await.unwrap();

            let binding = TablePartitionBinding {
                table_id: 0x42,
                rule_id: 0x55,
                ref_attr_indices: vec![1, 3],
            };
            write_partition_binding_to_catalog(&relation, &binding, 1)
                .await
                .unwrap();

            let loaded = load_partition_bindings_from_catalog(&relation)
                .await
                .unwrap();
            assert_eq!(loaded, vec![binding]);
        })
        .unwrap();
    }

    #[test]
    fn open_partition_binding_catalog_relation_metadata_matches_desc() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let path = catalog_path("metadata");
            let desc = partition_binding_catalog_desc().unwrap();
            let relation = open_partition_binding_catalog(&path, None).await.unwrap();

            assert_eq!(relation.table_id(), PARTITION_BINDING_CATALOG_TABLE_ID);
            assert_eq!(
                relation.partition_id(),
                PARTITION_BINDING_CATALOG_PARTITION_ID
            );
            assert_eq!(relation.table_id(), desc.id());
        })
        .unwrap();
    }
}
