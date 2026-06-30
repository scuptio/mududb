//! Basic consistency check for `SchemaTable` tuple descriptors.
#![allow(clippy::unwrap_used)]

use mudu_kernel::contract::schema_column::SchemaColumn;
use mudu_kernel::contract::schema_table::SchemaTable;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

#[test]
fn simple_schema_tuple_descs_match_columns() {
    let key_col = SchemaColumn::new(
        "id".to_string(),
        DatTypeID::I32,
        DatType::new_no_param(DatTypeID::I32).to_info(),
    );
    let val_col = SchemaColumn::new(
        "score".to_string(),
        DatTypeID::F64,
        DatType::new_no_param(DatTypeID::F64).to_info(),
    );
    let schema = SchemaTable::new("t".to_string(), vec![key_col, val_col], vec![0], vec![1]);

    let (key_desc, key_mapping) = schema.key_tuple_desc().unwrap();
    assert_eq!(key_desc.field_count(), 1);
    assert_eq!(key_mapping.len(), 1);
    assert!(schema.column_by_index(0).is_primary());

    let (val_desc, val_mapping) = schema.value_tuple_desc().unwrap();
    assert_eq!(val_desc.field_count(), 1);
    assert_eq!(val_mapping.len(), 1);
    assert!(!schema.column_by_index(1).is_primary());
}
