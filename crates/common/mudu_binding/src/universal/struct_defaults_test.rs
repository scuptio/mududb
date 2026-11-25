use super::mp_wire::{FromValue, ToValue, Value};
use super::uni_command_argv::UniCommandArgv;
use super::uni_data_value::UniDataValueField;
use super::uni_oid::UniOid;
use super::uni_procedure_param::UniProcedureParam;
use super::uni_procedure_result::UniProcedureResult;
use super::uni_query_argv::UniQueryArgv;
use super::uni_result_type::UniResultType;
use super::uni_sql_stmt::UniSqlStmt;
use super::uni_struct_type::{UniFieldAttr, UniStructField, UniStructType};
use super::uni_tuple_row::UniTupleRow;

fn assert_record_compatibility<T: Default + FromValue + ToValue>() {
    let expected = T::default().to_value();
    assert_eq!(
        T::from_value(&Value::Map(vec![])).unwrap().to_value(),
        expected
    );
    assert_eq!(T::from_value(&expected).unwrap().to_value(), expected);
    // Forward-compatible records ignore both unknown numeric tags and
    // nonnumeric extension keys, without interpreting their payloads.
    let extensions = Value::Map(vec![
        (Value::UInt(u64::MAX), Value::Bool(true)),
        (
            Value::Str("future-field".into()),
            Value::Array(vec![Value::Nil]),
        ),
    ]);
    assert_eq!(T::from_value(&extensions).unwrap().to_value(), expected);
    for invalid in [Value::Nil, Value::Array(vec![]), Value::Bool(false)] {
        assert!(T::from_value(&invalid).is_err());
    }
}

#[test]
fn missing_fields_default_and_unknown_fields_are_ignored() {
    assert_record_compatibility::<UniCommandArgv>();
    assert_record_compatibility::<UniQueryArgv>();
    assert_record_compatibility::<UniProcedureParam>();
    assert_record_compatibility::<UniProcedureResult>();
    assert_record_compatibility::<UniTupleRow>();
    assert_record_compatibility::<UniSqlStmt>();
    assert_record_compatibility::<UniResultType>();
}

#[test]
fn sql_statement_rejects_wrong_field_type_and_keeps_last_duplicate() {
    let invalid = Value::Map(vec![(Value::UInt(1), Value::Bool(true))]);
    assert!(UniSqlStmt::from_value(&invalid).is_err());
    let duplicate = Value::Map(vec![
        (Value::UInt(1), Value::Str("SELECT 1".into())),
        (Value::UInt(1), Value::Str("SELECT 2".into())),
    ]);
    assert_eq!(
        UniSqlStmt::from_value(&duplicate).unwrap().sql_string,
        "SELECT 2"
    );
}

fn assert_legacy_record<T: Default + serde::Serialize + serde::de::DeserializeOwned>() {
    let value = T::default();
    let expected = serde_json::to_value(&value).unwrap();
    let binary = rmp_serde::to_vec(&value).unwrap();
    let decoded: T = rmp_serde::from_slice(&binary).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
    let decoded: T = serde_json::from_str(r#"{"future": [1, {"x": true}]}"#).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
    // Sequence records require every positional field, unlike map defaults.
    let error = serde_json::from_str::<T>("[]").err().unwrap();
    assert!(error.to_string().contains("invalid length"));
    assert!(serde_json::from_str::<T>("false").is_err());
}

#[test]
fn legacy_records_keep_sequence_and_map_contracts() {
    assert_legacy_record::<UniOid>();
    assert_legacy_record::<UniFieldAttr>();
    assert_legacy_record::<UniStructField>();
    assert_legacy_record::<UniStructType>();
    assert_legacy_record::<UniResultType>();
    assert_legacy_record::<UniDataValueField>();
    assert_legacy_record::<UniProcedureResult>();
}

fn assert_invalid_variants<T: serde::de::DeserializeOwned>(tags: u32) {
    for invalid in ["null", "false", "{}", "[]", "[999,0]", "[\"bad\",0]"] {
        assert!(
            serde_json::from_str::<T>(invalid).is_err(),
            "accepted {invalid}"
        );
    }
    for tag in 0..tags {
        let missing_payload = format!("[{tag}]");
        assert!(
            serde_json::from_str::<T>(&missing_payload).is_err(),
            "accepted {missing_payload}"
        );
    }
}

#[test]
fn legacy_variants_reject_missing_payloads_unknown_tags_and_wrong_shapes() {
    assert_invalid_variants::<super::uni_data_type::UniDataType>(9);
    assert_invalid_variants::<super::uni_scalar_value::UniScalarValue>(22);
    assert_invalid_variants::<super::uni_data_value::UniDataValue>(4);
}
