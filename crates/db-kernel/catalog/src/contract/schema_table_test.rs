#![allow(clippy::unwrap_used)]

use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::{schema_columns_to_tuple_desc, SchemaTable};
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;

fn make_col(name: &str, ty: TypeFamily) -> SchemaColumn {
    SchemaColumn::new(name.to_string(), ty, DataType::new_no_param(ty).to_info())
}

#[test]
fn new_normalizes_key_and_value_indices() {
    let schema = SchemaTable::new(
        "t".to_string(),
        vec![
            make_col("k", TypeFamily::I32),
            make_col("v", TypeFamily::F64),
        ],
        vec![0],
        vec![1],
    );
    assert_eq!(schema.table_name(), "t");
    assert_eq!(schema.key_indices(), &vec![0]);
    assert_eq!(schema.value_indices(), &vec![1]);
    assert!(schema.column_by_index(0).is_primary());
    assert_eq!(schema.column_by_index(0).get_index(), 0);
    assert!(!schema.column_by_index(1).is_primary());
    assert_eq!(schema.column_by_index(1).get_index(), 0);
}

#[test]
fn key_and_value_column_views() {
    let schema = SchemaTable::new(
        "t2".to_string(),
        vec![
            make_col("a", TypeFamily::I64),
            make_col("b", TypeFamily::I32),
            make_col("c", TypeFamily::F64),
        ],
        vec![0, 1],
        vec![2],
    );
    let keys = schema.key_columns();
    let vals = schema.value_columns();
    assert_eq!(keys.len(), 2);
    assert_eq!(vals.len(), 1);
    assert_eq!(keys[0].get_name(), "a");
    assert_eq!(keys[1].get_name(), "b");
    assert_eq!(vals[0].get_name(), "c");
}

#[test]
fn tuple_descs_match_schema() {
    let schema = SchemaTable::new(
        "t3".to_string(),
        vec![
            make_col("id", TypeFamily::I32),
            make_col("score", TypeFamily::F64),
        ],
        vec![0],
        vec![1],
    );
    let (key_desc, key_mapping) = schema.key_tuple_desc().unwrap();
    assert_eq!(key_desc.field_count(), 1);
    assert_eq!(key_mapping.len(), 1);
    assert_eq!(key_mapping[0].datum_index(), 0);
    assert_eq!(key_mapping[0].column_index(), 0);

    let (val_desc, val_mapping) = schema.value_tuple_desc().unwrap();
    assert_eq!(val_desc.field_count(), 1);
    assert_eq!(val_mapping.len(), 1);
    assert_eq!(val_mapping[0].datum_index(), 0);
    assert_eq!(val_mapping[0].column_index(), 1);
}

#[test]
fn schema_columns_to_tuple_desc_maps_fields() {
    let schema = SchemaTable::new(
        "t4".to_string(),
        vec![
            make_col("x", TypeFamily::I32),
            make_col("y", TypeFamily::I32),
        ],
        vec![0],
        vec![1],
    );
    let cols = vec![
        (0, schema.column_by_index(0)),
        (1, schema.column_by_index(1)),
    ];
    let (desc, mapping) = schema_columns_to_tuple_desc(cols).unwrap();
    assert_eq!(desc.field_count(), 2);
    assert_eq!(mapping.len(), 2);
    let names: Vec<_> = mapping.iter().map(|f| f.name().as_str()).collect();
    assert!(names.contains(&"x"));
    assert!(names.contains(&"y"));
}

#[test]
fn serde_roundtrip_preserves_schema() {
    let schema = SchemaTable::new(
        "t5".to_string(),
        vec![
            make_col("k", TypeFamily::I32),
            make_col("v", TypeFamily::F64),
        ],
        vec![0],
        vec![1],
    );
    let json = serde_json::to_string(&schema).unwrap();
    let decoded: SchemaTable = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.table_name(), schema.table_name());
    assert_eq!(decoded.columns().len(), schema.columns().len());
    assert_eq!(decoded.id(), schema.id());
}
