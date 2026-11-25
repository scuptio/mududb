//! Unit tests for the SQL/datum conversion helpers in `sql.rs`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::sql::{
    data_value_to_sqlite_value, datum_binary_to_sqlite_value, sqlite_decl_data_type,
    sqlite_value_to_datum_binary, to_data_values, to_sqlite_values,
};
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use rusqlite::types::{Value, ValueRef};

#[test]
fn sqlite_decl_data_type_preserves_numeric_params() {
    let ty = sqlite_decl_data_type(Some("NUMERIC(18, 4)"), 0);
    assert_eq!(ty.type_family(), TypeFamily::Numeric);
    let numeric = ty.expect_numeric_param();
    assert_eq!(numeric.precision(), 18);
    assert_eq!(numeric.scale(), 4);

    // DECIMAL without explicit parameters uses the default numeric type.
    let ty = sqlite_decl_data_type(Some("DECIMAL"), 1);
    assert_eq!(ty.type_family(), TypeFamily::Numeric);

    // Malformed numeric parameters fall back instead of failing.
    let _ = sqlite_decl_data_type(Some("NUMERIC(x, 2)"), 0);
    let _ = sqlite_decl_data_type(Some("NUMERIC(18"), 0);

    let _ = sqlite_decl_data_type(Some("INTEGER"), 0);
    let _ = sqlite_decl_data_type(None, 3);
}

#[test]
fn data_value_to_sqlite_value_covers_supported_and_unsupported() {
    assert_eq!(
        data_value_to_sqlite_value(&DataValue::from_i32(1)).unwrap(),
        Value::Integer(1)
    );
    assert_eq!(
        data_value_to_sqlite_value(&DataValue::from_i64(2)).unwrap(),
        Value::Integer(2)
    );
    assert_eq!(
        data_value_to_sqlite_value(&DataValue::from_f32(1.5)).unwrap(),
        Value::Real(1.5)
    );
    assert_eq!(
        data_value_to_sqlite_value(&DataValue::from_f64(2.5)).unwrap(),
        Value::Real(2.5)
    );
    assert_eq!(
        data_value_to_sqlite_value(&DataValue::from_string("s".to_string())).unwrap(),
        Value::Text("s".to_string())
    );
    assert_eq!(
        data_value_to_sqlite_value(&DataValue::from_binary(vec![1, 2])).unwrap(),
        Value::Blob(vec![1, 2])
    );
    assert_eq!(
        data_value_to_sqlite_value(&DataValue::null()).unwrap(),
        Value::Null
    );
    // Numeric goes over its plain-string form.
    let numeric =
        DataValue::from_numeric(mudu::data_type::numeric::Numeric::parse("7.50").unwrap());
    assert_eq!(
        data_value_to_sqlite_value(&numeric).unwrap(),
        Value::Text("7.50".to_string())
    );

    // Types outside the chain are rejected.
    assert!(
        data_value_to_sqlite_value(&DataValue::from_array(vec![DataValue::from_i32(1)])).is_err()
    );
}

#[test]
fn datum_binary_roundtrips_through_sqlite_value() {
    let ty = DataType::new_no_param(TypeFamily::I32);
    let value = datum_binary_to_sqlite_value(&42i32.to_be_bytes(), &ty).unwrap();
    assert_eq!(value, Value::Integer(42));
    // A truncated datum fails the decode.
    assert!(datum_binary_to_sqlite_value(&[0, 1], &ty).is_err());

    let binary = sqlite_value_to_datum_binary(ValueRef::Integer(42), &ty).unwrap();
    assert_eq!(binary, Some(42i32.to_be_bytes().to_vec()));
    assert_eq!(
        sqlite_value_to_datum_binary(ValueRef::Null, &ty).unwrap(),
        None
    );
}

#[test]
fn params_convert_to_sqlite_and_wire_values() {
    let params = SQLParamValue::from_vec(vec![
        DataValue::from_i32(7),
        DataValue::from_string("x".to_string()),
    ]);
    let sqlite_values = to_sqlite_values(&params).unwrap();
    assert_eq!(sqlite_values.len(), 2);
    assert_eq!(sqlite_values[0], Value::Integer(7));

    let wire_values = to_data_values(&params).unwrap();
    assert_eq!(wire_values.len(), 2);
    assert_eq!(wire_values[0].as_i32(), Some(&7));
}
