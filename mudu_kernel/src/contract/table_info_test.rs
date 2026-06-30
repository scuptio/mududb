#![allow(clippy::unwrap_used)]

use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_info::TableInfo;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

fn make_col(name: &str, ty: DatTypeID) -> SchemaColumn {
    SchemaColumn::new(name.to_string(), ty, DatType::new_no_param(ty).to_info())
}

#[test]
fn new_builds_table_info() {
    let schema = SchemaTable::new(
        "info_t".to_string(),
        vec![
            make_col("id", DatTypeID::I32),
            make_col("val", DatTypeID::F64),
        ],
        vec![0],
        vec![1],
    );
    let info = TableInfo::new(schema).unwrap();
    let schema_arc = info.schema().unwrap();
    assert_eq!(schema_arc.table_name(), "info_t");
    assert_eq!(schema_arc.columns().len(), 2);

    let desc = info.table_desc().unwrap();
    assert_eq!(desc.name(), "info_t");
    assert_eq!(desc.fields().len(), 2);
    assert_eq!(desc.key_indices(), &vec![0]);
    assert_eq!(desc.value_indices(), &vec![1]);
}

#[test]
fn field_mapping_matches_columns() {
    let schema = SchemaTable::new(
        "info_t2".to_string(),
        vec![
            make_col("a", DatTypeID::I64),
            make_col("b", DatTypeID::I32),
            make_col("c", DatTypeID::F64),
        ],
        vec![0, 1],
        vec![2],
    );
    let info = TableInfo::new(schema).unwrap();
    let desc = info.table_desc().unwrap();
    let fields = desc.fields();
    assert_eq!(fields[0].name(), "a");
    assert_eq!(fields[0].column_index(), 0);
    assert!(fields[0].is_primary());
    assert_eq!(fields[2].name(), "c");
    assert_eq!(fields[2].column_index(), 2);
    assert!(!fields[2].is_primary());
}
