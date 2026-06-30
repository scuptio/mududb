#![allow(clippy::unwrap_used)]

use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_info::TableInfo;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

fn make_col(name: &str, ty: DatTypeID) -> SchemaColumn {
    SchemaColumn::new(name.to_string(), ty, DatType::new_no_param(ty).to_info())
}

fn sample_table() -> SchemaTable {
    SchemaTable::new(
        "desc_t".to_string(),
        vec![
            make_col("k1", DatTypeID::I32),
            make_col("k2", DatTypeID::I64),
            make_col("v1", DatTypeID::F64),
        ],
        vec![0, 1],
        vec![2],
    )
}

#[test]
fn table_desc_accessors_match_schema() {
    let info = TableInfo::new(sample_table()).unwrap();
    let desc = info.table_desc().unwrap();
    assert_eq!(desc.name(), "desc_t");
    assert_eq!(desc.key_field_oid().len(), 2);
    assert_eq!(desc.value_field_oid().len(), 1);
    assert_eq!(desc.key_indices(), &vec![0, 1]);
    assert_eq!(desc.value_indices(), &vec![2]);
    assert_eq!(desc.fields().len(), 3);
    assert_eq!(desc.get_attr(0).name(), "k1");
    assert_eq!(desc.get_attr(2).name(), "v1");
}

#[test]
fn key_and_value_info_views() {
    let info = TableInfo::new(sample_table()).unwrap();
    let desc = info.table_desc().unwrap();
    let keys = desc.key_info();
    let vals = desc.value_info();
    assert_eq!(keys.len(), 2);
    assert_eq!(vals.len(), 1);
    assert!(keys.iter().all(|f| f.is_primary()));
    assert!(!vals[0].is_primary());
}

#[test]
fn name_and_oid_maps() {
    let info = TableInfo::new(sample_table()).unwrap();
    let desc = info.table_desc().unwrap();
    let name2oid = desc.name2oid();
    let oid2col = desc.oid2col();
    let k1_oid = *name2oid.get("k1").unwrap();
    assert!(oid2col.contains_key(&k1_oid));
    assert_eq!(oid2col.get(&k1_oid).unwrap().name(), "k1");
    assert_eq!(desc.original_column_oid().len(), 3);
}

#[test]
fn tuple_descs_are_consistent() {
    let info = TableInfo::new(sample_table()).unwrap();
    let desc = info.table_desc().unwrap();
    assert_eq!(desc.key_desc().field_count(), 2);
    assert_eq!(desc.value_desc().field_count(), 1);
}
