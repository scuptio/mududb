#![allow(clippy::unwrap_used)]

use crate::contract::field_info::FieldInfo;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;

#[test]
fn new_and_accessors() {
    let ty = DataType::new_no_param(TypeFamily::I64);
    let f = FieldInfo::new("id".to_string(), 7, ty.clone(), 0, 1, Some(0), false);
    assert_eq!(f.name(), "id");
    assert_eq!(f.id(), 7);
    assert_eq!(f.column_index(), 1);
    assert_eq!(f.datum_index(), 0);
    assert!(f.is_primary());
    assert_eq!(f.primary_index(), Some(0));
    assert!(!f.nullable());
    assert_eq!(f.type_desc().type_family(), TypeFamily::I64);
    assert_eq!(f.type_desc().type_family(), ty.type_family());
}

#[test]
fn set_datum_index_updates() {
    let mut f = FieldInfo::new(
        "x".to_string(),
        0,
        DataType::new_no_param(TypeFamily::F64),
        0,
        0,
        None,
        true,
    );
    f.set_datum_index(3);
    assert_eq!(f.datum_index(), 3);
}

#[test]
fn default_field_info() {
    let f = FieldInfo::default();
    assert_eq!(f.name(), "");
    assert_eq!(f.id(), 0);
    assert_eq!(f.column_index(), 0);
    assert!(!f.is_primary());
    assert!(!f.nullable());
    assert_eq!(f.type_desc().type_family(), TypeFamily::I32);
}
