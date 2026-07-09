#![allow(clippy::unwrap_used)]

use crate::contract::schema_column::SchemaColumn;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;

#[test]
fn new_sets_fields_and_defaults() {
    let col = SchemaColumn::new(
        "id".to_string(),
        TypeFamily::I32,
        DataType::new_no_param(TypeFamily::I32).to_info(),
    );
    assert_eq!(col.get_name(), "id");
    assert_eq!(col.type_id(), TypeFamily::I32);
    assert!(col.nullable());
    assert!(!col.is_primary());
    assert_eq!(col.get_index(), 0);
    assert_ne!(col.get_oid(), 0);
}

#[test]
fn new_with_oid_preserves_oid() {
    let col = SchemaColumn::new_with_oid(
        42,
        "x".to_string(),
        TypeFamily::F64,
        DataType::new_no_param(TypeFamily::F64).to_info(),
    );
    assert_eq!(col.get_oid(), 42);
}

#[test]
fn primary_index_updates_state() {
    let mut col = SchemaColumn::new(
        "k".to_string(),
        TypeFamily::I64,
        DataType::new_no_param(TypeFamily::I64).to_info(),
    );
    assert!(!col.is_primary());
    col.set_primary_index(Some(0));
    assert!(col.is_primary());
    assert_eq!(col.primary_index(), Some(0));
    assert!(!col.nullable());
    col.set_primary_index(None);
    assert!(!col.is_primary());
}

#[test]
fn index_and_nullable_setters() {
    let mut col = SchemaColumn::new(
        "v".to_string(),
        TypeFamily::F64,
        DataType::new_no_param(TypeFamily::F64).to_info(),
    );
    col.set_index(3);
    assert_eq!(col.get_index(), 3);
    col.set_nullable(false);
    assert!(!col.nullable());
}

#[test]
fn fixed_length_matches_type() {
    let col_i32 = SchemaColumn::new(
        "a".to_string(),
        TypeFamily::I32,
        DataType::new_no_param(TypeFamily::I32).to_info(),
    );
    let col_f64 = SchemaColumn::new(
        "b".to_string(),
        TypeFamily::F64,
        DataType::new_no_param(TypeFamily::F64).to_info(),
    );
    assert!(col_i32.is_fixed_length());
    assert!(col_f64.is_fixed_length());
}

#[test]
fn serde_roundtrip_preserves_fields() {
    let col = SchemaColumn::new(
        "c".to_string(),
        TypeFamily::I64,
        DataType::new_no_param(TypeFamily::I64).to_info(),
    );
    let json = serde_json::to_string(&col).unwrap();
    let decoded: SchemaColumn = serde_json::from_str(&json).unwrap();
    assert_eq!(col.get_oid(), decoded.get_oid());
    assert_eq!(col.get_name(), decoded.get_name());
    assert_eq!(col.type_id(), decoded.type_id());
    assert_eq!(col.nullable(), decoded.nullable());
    assert_eq!(col.is_primary(), decoded.is_primary());
}
