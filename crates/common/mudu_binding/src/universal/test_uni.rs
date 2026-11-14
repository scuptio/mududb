#[cfg(test)]
pub(crate) mod tests {
    use crate::universal::mp_wire::{FromValue, ToValue, Value, decode_value, encode_value};
    use crate::universal::uni_command_argv::UniCommandArgv;
    use crate::universal::uni_data_type::UniDataType;
    use crate::universal::uni_data_value::{UniDataValue, UniDataValueField};
    use crate::universal::uni_error::UniError;
    use crate::universal::uni_fs_open_argv::UniFsOpenArgv;
    use crate::universal::uni_get_argv::UniGetArgv;
    use crate::universal::uni_get_result::UniGetResult;
    use crate::universal::uni_key_value::UniKeyValue;
    use crate::universal::uni_oid::UniOid;
    use crate::universal::uni_procedure_param::UniProcedureParam;
    use crate::universal::uni_procedure_result::UniProcedureResult;
    use crate::universal::uni_put_argv::UniPutArgv;
    use crate::universal::uni_put_result::UniPutResult;
    use crate::universal::uni_query_argv::UniQueryArgv;
    use crate::universal::uni_query_result::UniQueryResult;
    use crate::universal::uni_range_argv::UniRangeArgv;
    use crate::universal::uni_range_result::UniRangeResult;
    use crate::universal::uni_record_type::{UniFieldAttr, UniRecordField, UniRecordType};
    use crate::universal::uni_relation::{
        RELATION_DELTA_OP_ADD, RELATION_DELTA_OP_ADD_DEFERRED, RELATION_DELTA_OP_SUB,
        RELATION_DELTA_OP_SUB_DEFERRED, RELATION_DELTA_OP_SUB_WRAP_DEFERRED, UniRelationDelta,
    };
    use crate::universal::uni_result::UniResult;
    use crate::universal::uni_result_set::UniResultSet;
    use crate::universal::uni_result_type::UniResultType;
    use crate::universal::uni_scalar::UniScalar;
    use crate::universal::uni_scalar_value::UniScalarValue;
    use crate::universal::uni_sql_param::UniSqlParam;
    use crate::universal::uni_sql_stmt::UniSqlStmt;
    use crate::universal::uni_tuple_row::UniTupleRow;
    use crate::universal::uni_type_desc::UniTypeDesc;
    use mudu::common::serde_utils::{
        deserialize_from, deserialize_from_json, serialize_to_json, serialize_to_vec,
    };
    use mudu::data_type::numeric::Numeric;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use std::fmt::Debug;

    /// Roundtrip through the legacy JSON serde impls (kept for the types
    /// with JSON-facing consumers: the descriptor, the JSON client, the
    /// procedure HTTP API).
    pub(crate) fn assert_json_and_binary_roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + Clone + Debug + 'static,
    {
        let json = serialize_to_json(value).unwrap();
        let binary = serialize_to_vec(value).unwrap();

        let decoded_json: T = deserialize_from_json(json.as_str()).unwrap();
        let (decoded_binary, used): (T, u64) = deserialize_from(binary.as_slice()).unwrap();

        let json_after = serialize_to_json(&decoded_json).unwrap();
        let binary_after = serialize_to_vec(&decoded_binary).unwrap();

        assert_eq!(json_after, json);
        assert_eq!(binary_after, binary);
        assert_eq!(used as usize, binary.len());
    }

    /// Roundtrip through the MSSP wire runtime (`to_value` → canonical
    /// MessagePack → `from_value`), comparing the re-encoded bytes.
    pub(crate) fn assert_value_roundtrip<T>(value: &T)
    where
        T: ToValue + FromValue + Clone + Debug,
    {
        let binary = encode_value(&value.to_value());
        let decoded = T::from_value(&decode_value(&binary).unwrap()).unwrap();
        let binary_after = encode_value(&decoded.to_value());
        assert_eq!(binary_after, binary, "wire roundtrip failed for {value:?}");
    }

    fn sample_oid() -> UniOid {
        UniOid { h: 7, l: 42 }
    }

    fn sample_record_type() -> UniRecordType {
        UniRecordType {
            record_name: "vote_record".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "id".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::U128),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "name".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::String),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "tags".to_string(),
                    field_type: UniDataType::Array(Box::new(UniDataType::Scalar(
                        UniScalar::String,
                    ))),
                    field_attrs: Vec::new(),
                },
            ],
        }
    }

    fn sample_data_type() -> UniDataType {
        UniDataType::Record(UniRecordType {
            record_name: "envelope".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "meta".to_string(),
                    field_type: UniDataType::Tuple(vec![
                        UniDataType::Scalar(UniScalar::U64),
                        UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::String))),
                    ]),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "payload".to_string(),
                    field_type: UniDataType::Result(UniResultType {
                        ok: Some(Box::new(UniDataType::Array(Box::new(UniDataType::Scalar(
                            UniScalar::I32,
                        ))))),
                        err: Some(Box::new(UniDataType::Identifier("ErrCode".to_string()))),
                    }),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "blob".to_string(),
                    field_type: UniDataType::Binary,
                    field_attrs: Vec::new(),
                },
            ],
        })
    }

    fn sample_data_value() -> UniDataValue {
        UniDataValue::Record(vec![
            UniDataValueField {
                field_name: "scores".to_string(),
                field_value: UniDataValue::Array(vec![
                    UniDataValue::Scalar(UniScalarValue::from_i32(10)),
                    UniDataValue::Scalar(UniScalarValue::from_i32(-4)),
                ]),
            },
            UniDataValueField {
                field_name: "nested".to_string(),
                field_value: UniDataValue::Record(vec![
                    UniDataValueField {
                        field_name: "ok".to_string(),
                        field_value: UniDataValue::Scalar(UniScalarValue::from_bool(true)),
                    },
                    UniDataValueField {
                        field_name: "msg".to_string(),
                        field_value: UniDataValue::Scalar(UniScalarValue::from_string(
                            "ok".to_string(),
                        )),
                    },
                ]),
            },
            UniDataValueField {
                field_name: "payload".to_string(),
                field_value: UniDataValue::Scalar(UniScalarValue::from_blob(vec![1, 2, 3, 4, 200])),
            },
        ])
    }

    fn sample_query_result() -> UniQueryResult {
        UniQueryResult {
            tuple_desc: sample_record_type(),
            result_set: UniResultSet {
                eof: false,
                row_set: vec![UniTupleRow {
                    fields: vec![
                        UniDataValue::Scalar(UniScalarValue::from_u128(
                            99u128.to_be_bytes().to_vec(),
                        )),
                        UniDataValue::Scalar(UniScalarValue::from_string("alice".to_string())),
                        UniDataValue::Array(vec![
                            UniDataValue::Scalar(UniScalarValue::from_string("x".to_string())),
                            UniDataValue::Scalar(UniScalarValue::from_string("y".to_string())),
                        ]),
                    ],
                }],
                cursor: vec![9, 8, 7],
            },
        }
    }

    #[test]
    fn test_uni_data_type() {
        let uni_dat_ty = sample_data_type();
        assert_json_and_binary_roundtrip(&uni_dat_ty);

        let json = serialize_to_json(&uni_dat_ty).unwrap();
        let uni_dat_ty2: UniDataType = deserialize_from_json(json.as_str()).unwrap();
        let record = uni_dat_ty2.as_record().expect("record dat type");
        assert_eq!(record.record_name, "envelope");
        assert_eq!(record.record_fields.len(), 3);
        assert!(record.record_fields[2].field_type.as_identifier().is_none());
    }

    #[test]
    fn test_uni_scalar_value_roundtrip_matrix() {
        let cases = vec![
            UniScalarValue::from_bool(true),
            UniScalarValue::from_u8(3),
            UniScalarValue::from_i8(7),
            UniScalarValue::from_u16(16),
            UniScalarValue::from_i16(-16),
            UniScalarValue::from_u32(32),
            UniScalarValue::from_i32(-32),
            UniScalarValue::from_u64(64),
            UniScalarValue::from_u128(128u128.to_be_bytes().to_vec()),
            UniScalarValue::from_i64(-64),
            UniScalarValue::from_i128((-128i128).to_be_bytes().to_vec()),
            UniScalarValue::from_f32(3.25),
            UniScalarValue::from_f64(-9.5),
            UniScalarValue::from_char('z'),
            UniScalarValue::from_string("hello".to_string()),
            UniScalarValue::from_blob(vec![0, 1, 2, 255]),
            UniScalarValue::from_numeric("12.3400".to_string()),
        ];

        for value in cases {
            assert_json_and_binary_roundtrip(&value);
        }
    }

    #[test]
    fn test_uni_data_value_roundtrip_matrix() {
        let cases = vec![
            UniDataValue::Scalar(UniScalarValue::from_string("row".to_string())),
            UniDataValue::Array(vec![
                UniDataValue::Scalar(UniScalarValue::from_u64(1)),
                UniDataValue::Scalar(UniScalarValue::from_u64(2)),
            ]),
            sample_data_value(),
            UniDataValue::Binary(vec![0, 1, 2, 3, 255]),
        ];

        for value in cases {
            assert_json_and_binary_roundtrip(&value);
        }
    }

    #[test]
    fn test_uni_result_roundtrip_for_ok_and_err() {
        let ok: UniResult<UniDataType, UniError> = UniResult::Ok(sample_data_type());
        let err: UniResult<UniDataType, UniError> = UniResult::Err(UniError {
            err_code: 404,
            err_msg: "not found".to_string(),
            err_src: "unit-test".to_string(),
            err_loc: "test_uni".to_string(),
            err_details: vec![1, 2, 3],
        });

        assert_value_roundtrip(&ok);
        assert_value_roundtrip(&err);
    }

    #[test]
    fn test_universal_request_and_result_struct_roundtrip() {
        let sql_stmt = UniSqlStmt {
            sql_string: "select id, name from users where id = ?".to_string(),
        };
        let sql_param = UniSqlParam {
            params: vec![UniDataValue::Scalar(UniScalarValue::from_u128(
                7u128.to_be_bytes().to_vec(),
            ))],
            param_names: None,
        };

        let query_argv = UniQueryArgv {
            oid: sample_oid(),
            query: sql_stmt.clone(),
            param_list: sql_param.clone(),
            param_desc: None,
        };
        let command_argv = UniCommandArgv {
            oid: sample_oid(),
            command: sql_stmt,
            param_list: sql_param,
            param_desc: None,
        };
        let procedure_param = UniProcedureParam {
            procedure: 88,
            session: sample_oid(),
            param_list: vec![sample_data_value()],
        };
        let procedure_result = UniProcedureResult {
            return_list: vec![sample_data_value()],
        };
        let get_result = UniGetResult {
            value: Some(UniDataValue::Scalar(UniScalarValue::from_string(
                "payload".to_string(),
            ))),
        };
        let range_result = UniRangeResult {
            items: vec![UniKeyValue {
                key: UniDataValue::Scalar(UniScalarValue::from_u64(1)),
                value: sample_data_value(),
            }],
        };
        let query_result = sample_query_result();

        assert_value_roundtrip(&query_argv);
        assert_value_roundtrip(&command_argv);
        assert_value_roundtrip(&procedure_param);
        assert_value_roundtrip(&procedure_result);
        assert_value_roundtrip(&get_result);
        assert_value_roundtrip(&range_result);
        assert_value_roundtrip(&query_result);
        assert_value_roundtrip(&sample_oid());
    }

    #[test]
    fn test_uni_data_type_and_value_reject_invalid_tags() {
        let invalid_data_type_json = "[99,0]";
        let invalid_data_value_json = "[99,0]";
        let invalid_scalar_json = "[99,0]";

        assert!(deserialize_from_json::<UniDataType>(invalid_data_type_json).is_err());
        assert!(deserialize_from_json::<UniDataValue>(invalid_data_value_json).is_err());
        assert!(deserialize_from_json::<UniScalarValue>(invalid_scalar_json).is_err());

        // The wire conversion rejects unknown variant tags as well.
        let bad = Value::Array(vec![Value::from(99u32), Value::from(0u8)]);
        assert!(UniDataType::from_value(&bad).is_err());
        assert!(UniDataValue::from_value(&bad).is_err());
        assert!(UniScalarValue::from_value(&bad).is_err());
    }

    #[test]
    fn test_uni_result_rejects_invalid_payload_shape() {
        // Not a one-entry map.
        let object = Value::Map(vec![]);
        assert!(UniResult::<UniDataType, UniError>::from_value(&object).is_err());
        let unknown_tag = Value::Map(vec![(Value::from(99u32), Value::Map(vec![]))]);
        assert!(UniResult::<UniDataType, UniError>::from_value(&unknown_tag).is_err());
    }

    #[test]
    fn test_uni_data_type_binary_rejects_truncated_payload() {
        let binary = encode_value(&sample_data_type().to_value());
        let truncated = &binary[..binary.len() - 1];
        assert!(decode_value(truncated).is_err());
    }

    #[test]
    fn test_uni_numeric_type_params_roundtrip_through_data_type() {
        let uni_ty = UniDataType::Scalar(UniScalar::Numeric);
        let params = vec![
            UniDataValue::Scalar(UniScalarValue::from_i64(18)),
            UniDataValue::Scalar(UniScalarValue::from_i64(4)),
        ];

        let data_type = uni_ty.clone().uni_to_with_params(Some(params)).unwrap();
        assert_eq!(
            data_type.type_family(),
            mudu_type::type_family::TypeFamily::Numeric
        );
        let numeric = data_type.expect_numeric_param();
        assert_eq!(numeric.precision(), 18);
        assert_eq!(numeric.scale(), 4);

        let uni_back = UniDataType::uni_from(data_type).unwrap();
        assert_eq!(
            serialize_to_json(&uni_back).unwrap(),
            serialize_to_json(&uni_ty).unwrap()
        );
    }

    #[test]
    fn test_uni_numeric_value_roundtrip_through_data_value() {
        let uni_value = UniDataValue::Scalar(UniScalarValue::from_numeric("12.3400".to_string()));

        let data_value = uni_value.clone().uni_to().unwrap();
        assert_eq!(data_value.expect_numeric().to_plain_string(), "12.3400");

        let uni_back = UniDataValue::uni_from(data_value).unwrap();
        let scalar = uni_back.as_scalar().expect("scalar numeric");
        assert_eq!(scalar.expect_numeric(), "12.3400");

        assert_json_and_binary_roundtrip(&uni_value);
    }

    #[test]
    fn test_uni_numeric_rejects_invalid_shapes() {
        let invalid_numeric_value =
            UniDataValue::Scalar(UniScalarValue::from_numeric("not-a-number".to_string()));
        assert!(invalid_numeric_value.uni_to().is_err());

        let invalid_numeric_type = UniDataType::Scalar(UniScalar::Numeric);
        let invalid_params = vec![
            UniDataValue::Scalar(UniScalarValue::from_i64(4)),
            UniDataValue::Scalar(UniScalarValue::from_i64(9)),
        ];
        assert!(
            invalid_numeric_type
                .uni_to_with_params(Some(invalid_params))
                .is_err()
        );

        let data_type = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let uni_type = UniDataType::uni_from(data_type).unwrap();
        assert_eq!(uni_type.as_scalar(), Some(&UniScalar::Numeric));

        let data_value =
            mudu_type::data_value::DataValue::from_numeric(Numeric::parse("7.50").unwrap());
        let uni_value = UniDataValue::uni_from(data_value).unwrap();
        let scalar = uni_value.as_scalar().expect("numeric scalar");
        assert_eq!(scalar.expect_numeric(), "7.50");
    }

    type TemporalTypeCase = (
        UniDataType,
        Vec<UniDataValue>,
        mudu_type::type_family::TypeFamily,
        fn(&DataType) -> u8,
        u8,
    );

    #[test]
    fn test_uni_temporal_type_params_roundtrip_through_data_type() {
        let cases: Vec<TemporalTypeCase> = vec![
            (
                UniDataType::Scalar(UniScalar::Time),
                vec![UniDataValue::Scalar(UniScalarValue::from_i64(3))],
                mudu_type::type_family::TypeFamily::Time,
                |dt: &DataType| dt.expect_time_param().precision(),
                3u8,
            ),
            (
                UniDataType::Scalar(UniScalar::Timestamp),
                vec![UniDataValue::Scalar(UniScalarValue::from_i64(4))],
                mudu_type::type_family::TypeFamily::Timestamp,
                |dt: &DataType| dt.expect_timestamp_param().precision(),
                4u8,
            ),
            (
                UniDataType::Scalar(UniScalar::TimestampTz),
                vec![UniDataValue::Scalar(UniScalarValue::from_i64(2))],
                mudu_type::type_family::TypeFamily::TimestampTz,
                |dt: &DataType| dt.expect_timestamptz_param().precision(),
                2u8,
            ),
        ];

        for (uni_ty, params, expected_id, precision_of, expected_precision) in cases {
            let data_type = uni_ty.clone().uni_to_with_params(Some(params)).unwrap();
            assert_eq!(data_type.type_family(), expected_id);
            assert_eq!(precision_of(&data_type), expected_precision);

            let uni_back = UniDataType::uni_from(data_type).unwrap();
            assert_eq!(
                serialize_to_json(&uni_back).unwrap(),
                serialize_to_json(&uni_ty).unwrap()
            );
        }
    }

    type TemporalValueCase = (UniDataValue, &'static str, fn(&UniScalarValue) -> &str);

    #[test]
    fn test_uni_temporal_values_roundtrip_through_data_value() {
        let cases: Vec<TemporalValueCase> = vec![
            (
                UniDataValue::Scalar(UniScalarValue::from_date("2026-05-20".to_string())),
                "2026-05-20",
                |s: &UniScalarValue| s.expect_date().as_str(),
            ),
            (
                UniDataValue::Scalar(UniScalarValue::from_time("12:34:56.123456".to_string())),
                "12:34:56.123456",
                |s: &UniScalarValue| s.expect_time().as_str(),
            ),
            (
                UniDataValue::Scalar(UniScalarValue::from_timestamp(
                    "2026-05-20 14:30:45.123456".to_string(),
                )),
                "2026-05-20 14:30:45.123456",
                |s: &UniScalarValue| s.expect_timestamp().as_str(),
            ),
            (
                UniDataValue::Scalar(UniScalarValue::from_timestamp_tz(
                    "2026-05-20T14:30:45.123456+08:00".to_string(),
                )),
                "2026-05-20 06:30:45.123456+00:00",
                |s: &UniScalarValue| s.expect_timestamp_tz().as_str(),
            ),
        ];

        for (uni_value, expected_text, text_of) in cases {
            let data_value = uni_value.clone().uni_to().unwrap();
            let uni_back = UniDataValue::uni_from(data_value).unwrap();
            let scalar = uni_back.as_scalar().expect("temporal scalar");
            assert_eq!(text_of(scalar), expected_text);
            assert_json_and_binary_roundtrip(&uni_value);
        }
    }

    #[test]
    fn test_uni_temporal_rejects_invalid_shapes() {
        let invalid_values = vec![
            UniDataValue::Scalar(UniScalarValue::from_date("2026-02-30".to_string())),
            UniDataValue::Scalar(UniScalarValue::from_time("25:00:00".to_string())),
            UniDataValue::Scalar(UniScalarValue::from_timestamp(
                "not-a-timestamp".to_string(),
            )),
            UniDataValue::Scalar(UniScalarValue::from_timestamp_tz(
                "2026-05-20 14:30:45".to_string(),
            )),
        ];
        for value in invalid_values {
            assert!(value.uni_to().is_err());
        }
    }

    #[test]
    fn test_uni_kv_and_fs_argv_wire_roundtrip() {
        let get_argv = UniGetArgv {
            oid: sample_oid(),
            key: UniDataValue::Scalar(UniScalarValue::from_string("k".to_string())),
        };
        let put_argv = UniPutArgv {
            oid: sample_oid(),
            key: UniDataValue::Scalar(UniScalarValue::from_string("k".to_string())),
            value: sample_data_value(),
        };
        let range_argv = UniRangeArgv {
            oid: sample_oid(),
            start_key: UniDataValue::Scalar(UniScalarValue::from_u64(1)),
            end_key: UniDataValue::Scalar(UniScalarValue::from_u64(9)),
        };
        let put_result = UniPutResult { ok: true };
        let fs_open_argv = UniFsOpenArgv {
            session: sample_oid(),
            oid: sample_oid(),
            path: "/tmp/data".to_string(),
            flags: 0b11,
        };

        assert_value_roundtrip(&get_argv);
        assert_value_roundtrip(&put_argv);
        assert_value_roundtrip(&range_argv);
        assert_value_roundtrip(&put_result);
        assert_value_roundtrip(&fs_open_argv);
    }

    #[test]
    fn test_uni_fs_open_argv_default() {
        let argv = UniFsOpenArgv::default();
        assert_eq!(argv.session.h, 0);
        assert_eq!(argv.oid.h, 0);
        assert!(argv.path.is_empty());
        assert_eq!(argv.flags, 0);
    }

    #[test]
    fn test_uni_argv_from_value_skips_unknown_keys_and_defaults() {
        // Non-numeric keys are skipped, unknown field numbers are ignored,
        // and missing fields fall back to defaults.
        let messy = Value::Map(vec![
            (Value::from("not-a-number"), Value::from(0u32)),
            (Value::from(99u32), Value::from(0u32)),
            (Value::from(4u32), Value::from(7u32)),
        ]);

        let fs_open = UniFsOpenArgv::from_value(&messy).unwrap();
        assert_eq!(fs_open.flags, 7);
        assert!(fs_open.path.is_empty());
        assert_eq!(fs_open.session.l, 0);

        let get_argv = UniGetArgv::from_value(&messy).unwrap();
        assert_eq!(get_argv.oid.h, 0);

        let put_argv = UniPutArgv::from_value(&messy).unwrap();
        assert_eq!(put_argv.oid.l, 0);

        let range_argv = UniRangeArgv::from_value(&messy).unwrap();
        assert_eq!(range_argv.oid.h, 0);

        // `UniPutResult` has no field 4, so nothing is picked up here.
        let put_result = UniPutResult::from_value(&messy).unwrap();
        assert!(!put_result.ok);

        // A non-map payload is rejected by every one of them.
        let not_a_map = Value::from(1u32);
        assert!(UniGetArgv::from_value(&not_a_map).is_err());
        assert!(UniPutArgv::from_value(&not_a_map).is_err());
        assert!(UniRangeArgv::from_value(&not_a_map).is_err());
        assert!(UniPutResult::from_value(&not_a_map).is_err());
        assert!(UniFsOpenArgv::from_value(&not_a_map).is_err());
    }

    #[test]
    fn test_uni_argv_from_value_rejects_malformed_fields() {
        // A known field number carrying a payload of the wrong shape fails.
        let bad_first = Value::Map(vec![(Value::from(1u32), Value::from("oops"))]);
        assert!(UniGetArgv::from_value(&bad_first).is_err());
        assert!(UniPutArgv::from_value(&bad_first).is_err());
        assert!(UniRangeArgv::from_value(&bad_first).is_err());
        assert!(UniPutResult::from_value(&bad_first).is_err());
        assert!(UniFsOpenArgv::from_value(&bad_first).is_err());

        let bad_second = Value::Map(vec![(Value::from(2u32), Value::from("oops"))]);
        assert!(UniGetArgv::from_value(&bad_second).is_err());
        assert!(UniPutArgv::from_value(&bad_second).is_err());
        assert!(UniRangeArgv::from_value(&bad_second).is_err());
        assert!(UniFsOpenArgv::from_value(&bad_second).is_err());

        let bad_third = Value::Map(vec![(Value::from(3u32), Value::from(1u32))]);
        assert!(UniPutArgv::from_value(&bad_third).is_err());
        assert!(UniRangeArgv::from_value(&bad_third).is_err());
        assert!(UniFsOpenArgv::from_value(&bad_third).is_err());

        let bad_flags = Value::Map(vec![(Value::from(4u32), Value::from("oops"))]);
        assert!(UniFsOpenArgv::from_value(&bad_flags).is_err());
    }

    pub(crate) fn scalar_value_samples() -> Vec<UniScalarValue> {
        vec![
            UniScalarValue::from_bool(true),
            UniScalarValue::from_u8(3),
            UniScalarValue::from_i8(-3),
            UniScalarValue::from_u16(16),
            UniScalarValue::from_i16(-16),
            UniScalarValue::from_u32(32),
            UniScalarValue::from_i32(-32),
            UniScalarValue::from_u64(64),
            UniScalarValue::from_u128(128u128.to_be_bytes().to_vec()),
            UniScalarValue::from_i64(-64),
            UniScalarValue::from_i128((-128i128).to_be_bytes().to_vec()),
            UniScalarValue::from_f32(3.25),
            UniScalarValue::from_f64(-9.5),
            UniScalarValue::from_char('z'),
            UniScalarValue::from_string("hello".to_string()),
            UniScalarValue::from_blob(vec![0, 1, 2, 255]),
            UniScalarValue::from_numeric("12.3400".to_string()),
            UniScalarValue::from_date("2026-05-20".to_string()),
            UniScalarValue::from_time("12:34:56".to_string()),
            UniScalarValue::from_timestamp("2026-05-20 14:30:45".to_string()),
            UniScalarValue::from_timestamp_tz("2026-05-20T14:30:45+08:00".to_string()),
            UniScalarValue::Null,
        ]
    }

    pub(crate) fn assert_panics(f: impl FnOnce() + std::panic::UnwindSafe) {
        assert!(std::panic::catch_unwind(f).is_err());
    }

    #[test]
    fn test_uni_scalar_value_from_value_error_shapes() {
        // Not an array, wrong element count, non-numeric tag.
        assert!(UniScalarValue::from_value(&Value::from(1u32)).is_err());
        assert!(UniScalarValue::from_value(&Value::Array(vec![Value::from(0u32)])).is_err());
        assert!(
            UniScalarValue::from_value(&Value::Array(vec![Value::from("x"), Value::from(0u8)]))
                .is_err()
        );

        // Well-formed tag with a payload of the wrong type.
        assert!(
            UniScalarValue::from_value(&Value::Array(vec![Value::from(0u32), Value::from(1u32)]))
                .is_err()
        );
        assert!(
            UniScalarValue::from_value(&Value::Array(vec![Value::from(8u32), Value::from(1u32)]))
                .is_err()
        );
        assert!(
            UniScalarValue::from_value(&Value::Array(vec![Value::from(14u32), Value::from(1u32)]))
                .is_err()
        );
    }

    #[test]
    fn test_uni_scalar_discriminant_wire_roundtrip() {
        let scalars = [
            UniScalar::Bool,
            UniScalar::U8,
            UniScalar::I8,
            UniScalar::U16,
            UniScalar::I16,
            UniScalar::U32,
            UniScalar::I32,
            UniScalar::U64,
            UniScalar::U128,
            UniScalar::I64,
            UniScalar::I128,
            UniScalar::F32,
            UniScalar::F64,
            UniScalar::Char,
            UniScalar::String,
            UniScalar::Blob,
            UniScalar::Numeric,
            UniScalar::Date,
            UniScalar::Time,
            UniScalar::Timestamp,
            UniScalar::TimestampTz,
        ];
        for scalar in scalars {
            assert_value_roundtrip(&scalar);
        }
        assert!(UniScalar::from_value(&Value::from(99u32)).is_err());
        assert!(UniScalar::from_value(&Value::from("x")).is_err());
    }

    #[test]
    fn test_uni_sql_param_wire_edges() {
        let default_param = UniSqlParam::default();
        assert!(default_param.params.is_empty());
        assert!(default_param.param_names.is_none());

        let named = UniSqlParam {
            params: vec![UniDataValue::Scalar(UniScalarValue::from_u8(1))],
            param_names: Some(vec!["p1".to_string()]),
        };
        assert_value_roundtrip(&named);

        // Non-numeric keys are skipped, unknown field numbers are ignored.
        let messy = Value::Map(vec![
            (Value::from("x"), Value::from(0u32)),
            (Value::from(99u32), Value::from(0u32)),
        ]);
        let parsed = UniSqlParam::from_value(&messy).unwrap();
        assert!(parsed.params.is_empty());

        // Malformed payloads on known fields are rejected.
        assert!(UniSqlParam::from_value(&Value::from(1u32)).is_err());
        let bad_params = Value::Map(vec![(Value::from(1u32), Value::from(1u32))]);
        assert!(UniSqlParam::from_value(&bad_params).is_err());
        let bad_names = Value::Map(vec![(Value::from(2u32), Value::from(1u32))]);
        assert!(UniSqlParam::from_value(&bad_names).is_err());
    }

    #[test]
    fn test_uni_record_type_and_field_attr_wire() {
        let default_attr = UniFieldAttr::default();
        assert!(default_attr.attr_name.is_empty());
        let default_field = UniRecordField::default();
        assert!(default_field.field_name.is_empty());
        let default_record = UniRecordType::default();
        assert!(default_record.record_name.is_empty());

        let record = UniRecordType {
            record_name: "r".to_string(),
            record_fields: vec![UniRecordField {
                field_name: "f".to_string(),
                field_type: UniDataType::Scalar(UniScalar::U8),
                field_attrs: vec![UniFieldAttr {
                    attr_name: "a".to_string(),
                    attr_value: "v".to_string(),
                }],
            }],
        };
        assert_value_roundtrip(&record);

        // Missing fields fall back to defaults; unknown keys are ignored.
        let empty_map = Value::Map(vec![]);
        assert!(
            UniRecordType::from_value(&empty_map)
                .unwrap()
                .record_fields
                .is_empty()
        );
        assert!(
            UniRecordField::from_value(&empty_map)
                .unwrap()
                .field_name
                .is_empty()
        );
        assert!(
            UniFieldAttr::from_value(&empty_map)
                .unwrap()
                .attr_name
                .is_empty()
        );
        let messy = Value::Map(vec![
            (Value::from("x"), Value::from(0u32)),
            (Value::from(99u32), Value::from(0u32)),
        ]);
        assert!(UniRecordType::from_value(&messy).is_ok());
        assert!(UniRecordField::from_value(&messy).is_ok());
        assert!(UniFieldAttr::from_value(&messy).is_ok());

        // Non-map payloads and malformed known fields are rejected.
        assert!(UniRecordType::from_value(&Value::from(1u32)).is_err());
        assert!(UniRecordField::from_value(&Value::from(1u32)).is_err());
        assert!(UniFieldAttr::from_value(&Value::from(1u32)).is_err());
        let bad_first = Value::Map(vec![(Value::from(1u32), Value::from(1u32))]);
        assert!(UniRecordType::from_value(&bad_first).is_err());
        assert!(UniFieldAttr::from_value(&bad_first).is_err());
        let bad_second = Value::Map(vec![(Value::from(2u32), Value::from(1u32))]);
        assert!(UniRecordField::from_value(&bad_second).is_err());
        assert!(UniRecordType::from_value(&bad_second).is_err());
        let bad_third = Value::Map(vec![(Value::from(3u32), Value::from(1u32))]);
        assert!(UniRecordField::from_value(&bad_third).is_err());
    }

    #[test]
    fn test_uni_relation_delta_constructors() {
        let add = UniRelationDelta::add(1, vec![1]);
        assert_eq!(add.attr, 1);
        assert_eq!(add.op, RELATION_DELTA_OP_ADD);
        assert_eq!(add.datum, vec![1]);

        let sub = UniRelationDelta::sub(2, vec![2]);
        assert_eq!(sub.op, RELATION_DELTA_OP_SUB);

        let add_deferred = UniRelationDelta::add_deferred(3, vec![3]);
        assert_eq!(add_deferred.op, RELATION_DELTA_OP_ADD_DEFERRED);

        let sub_deferred = UniRelationDelta::sub_deferred(4, vec![4]);
        assert_eq!(sub_deferred.op, RELATION_DELTA_OP_SUB_DEFERRED);

        let wrap = UniRelationDelta::sub_wrap(5, 7, 10, 100);
        assert_eq!(wrap.attr, 5);
        assert_eq!(wrap.op, RELATION_DELTA_OP_SUB_WRAP_DEFERRED);
        let mut expected_datum = Vec::new();
        expected_datum.extend_from_slice(&7i64.to_be_bytes());
        expected_datum.extend_from_slice(&10i64.to_be_bytes());
        expected_datum.extend_from_slice(&100i64.to_be_bytes());
        assert_eq!(wrap.datum, expected_datum);
    }

    #[test]
    fn test_uni_result_conversions_and_error_edges() {
        let ok: UniResult<UniDataType, UniError> = UniResult::Ok(sample_data_type());
        let err: UniResult<UniDataType, UniError> = UniResult::Err(UniError {
            err_code: 500,
            err_msg: "boom".to_string(),
            err_src: "unit-test".to_string(),
            err_loc: "test_uni".to_string(),
            err_details: vec![],
        });

        // `From` conversions in both directions.
        let ok_as_result: Result<UniDataType, UniError> = Result::from(ok.clone());
        assert!(ok_as_result.is_ok());
        let err_as_result: Result<UniDataType, UniError> = Result::from(err.clone());
        assert!(err_as_result.is_err());
        let ok_from_result = UniResult::<UniDataType, UniError>::from(Ok(sample_data_type()));
        assert!(matches!(ok_from_result, UniResult::Ok(_)));
        let err_from_result =
            UniResult::<UniDataType, UniError>::from(Err(err_as_result.unwrap_err()));
        assert!(matches!(err_from_result, UniResult::Err(_)));

        // `map_err` transforms only the error variant.
        let mapped_ok = ok.map_err(|e| e.err_code);
        assert!(Result::<UniDataType, u32>::from(mapped_ok).is_ok());
        let mapped_err = err.map_err(|e| e.err_code);
        let code = Result::<UniDataType, u32>::from(mapped_err).unwrap_err();
        assert_eq!(code, 500);

        // from_value error edges: non-map, non-numeric key, bad payloads.
        assert!(UniResult::<UniDataType, UniError>::from_value(&Value::from(1u32)).is_err());
        let bad_key = Value::Map(vec![(Value::from("x"), Value::Map(vec![]))]);
        assert!(UniResult::<UniDataType, UniError>::from_value(&bad_key).is_err());
        let bad_ok_payload = Value::Map(vec![(Value::from(0u32), Value::from(1u32))]);
        assert!(UniResult::<UniDataType, UniError>::from_value(&bad_ok_payload).is_err());
        let bad_err_payload = Value::Map(vec![(Value::from(1u32), Value::from(1u32))]);
        assert!(UniResult::<UniDataType, UniError>::from_value(&bad_err_payload).is_err());
    }

    #[test]
    fn test_uni_type_desc_extend() {
        let mut desc = UniTypeDesc::default();
        desc.types
            .insert("t1".to_string(), UniDataType::Scalar(UniScalar::U8));
        let mut other = UniTypeDesc::default();
        other
            .types
            .insert("t2".to_string(), UniDataType::Scalar(UniScalar::String));
        desc.extend(other);
        assert_eq!(desc.types.len(), 2);
        assert_json_and_binary_roundtrip(&desc);
    }
}
