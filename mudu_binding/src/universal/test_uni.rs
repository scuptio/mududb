#[cfg(test)]
mod tests {
    use crate::universal::uni_command_argv::UniCommandArgv;
    use crate::universal::uni_data_type::UniDataType;
    use crate::universal::uni_data_value::{UniDataValue, UniDataValueField};
    use crate::universal::uni_error::UniError;
    use crate::universal::uni_get_result::UniGetResult;
    use crate::universal::uni_key_value::UniKeyValue;
    use crate::universal::uni_oid::UniOid;
    use crate::universal::uni_procedure_param::UniProcedureParam;
    use crate::universal::uni_procedure_result::UniProcedureResult;
    use crate::universal::uni_query_argv::UniQueryArgv;
    use crate::universal::uni_query_result::UniQueryResult;
    use crate::universal::uni_range_result::UniRangeResult;
    use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
    use crate::universal::uni_result::UniResult;
    use crate::universal::uni_result_set::UniResultSet;
    use crate::universal::uni_result_type::UniResultType;
    use crate::universal::uni_scalar::UniScalar;
    use crate::universal::uni_scalar_value::UniScalarValue;
    use crate::universal::uni_sql_param::UniSqlParam;
    use crate::universal::uni_sql_stmt::UniSqlStmt;
    use crate::universal::uni_tuple_row::UniTupleRow;
    use mudu::common::serde_utils::{
        deserialize_from, deserialize_from_json, serialize_to_json, serialize_to_vec,
    };
    use mudu::data_type::numeric::Numeric;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use std::fmt::Debug;

    fn assert_json_and_binary_roundtrip<T>(value: &T)
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
                        UniDataValue::Scalar(UniScalarValue::from_u128(99)),
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
            UniScalarValue::from_u128(128),
            UniScalarValue::from_i64(-64),
            UniScalarValue::from_i128(-128),
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

        assert_json_and_binary_roundtrip(&ok);
        assert_json_and_binary_roundtrip(&err);
    }

    #[test]
    fn test_universal_request_and_result_struct_roundtrip() {
        let sql_stmt = UniSqlStmt {
            sql_string: "select id, name from users where id = ?".to_string(),
        };
        let sql_param = UniSqlParam {
            params: vec![UniDataValue::Scalar(UniScalarValue::from_u128(7))],
        };

        let query_argv = UniQueryArgv {
            oid: sample_oid(),
            query: sql_stmt.clone(),
            param_list: sql_param.clone(),
        };
        let command_argv = UniCommandArgv {
            oid: sample_oid(),
            command: sql_stmt,
            param_list: sql_param,
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

        assert_json_and_binary_roundtrip(&query_argv);
        assert_json_and_binary_roundtrip(&command_argv);
        assert_json_and_binary_roundtrip(&procedure_param);
        assert_json_and_binary_roundtrip(&procedure_result);
        assert_json_and_binary_roundtrip(&get_result);
        assert_json_and_binary_roundtrip(&range_result);
        assert_json_and_binary_roundtrip(&query_result);
    }

    #[test]
    fn test_uni_data_type_and_value_reject_invalid_tags() {
        let invalid_data_type_json = "[99,0]";
        let invalid_data_value_json = "[99,0]";
        let invalid_scalar_json = "[99,0]";

        assert!(deserialize_from_json::<UniDataType>(invalid_data_type_json).is_err());
        assert!(deserialize_from_json::<UniDataValue>(invalid_data_value_json).is_err());
        assert!(deserialize_from_json::<UniScalarValue>(invalid_scalar_json).is_err());
    }

    #[test]
    fn test_uni_result_rejects_invalid_payload_shape() {
        assert!(deserialize_from_json::<UniResult<UniDataType, UniError>>("{}").is_err());
        assert!(deserialize_from_json::<UniResult<UniDataType, UniError>>("{\"99\":{}}").is_err());
    }

    #[test]
    fn test_uni_data_type_binary_rejects_truncated_payload() {
        let binary = serialize_to_vec(&sample_data_type()).unwrap();
        let truncated = &binary[..binary.len() - 1];
        assert!(deserialize_from::<UniDataType>(truncated).is_err());
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
        assert!(matches!(uni_type, UniDataType::Scalar(UniScalar::Numeric)));

        let data_value =
            mudu_type::data_value::DataValue::from_numeric(Numeric::parse("7.50").unwrap());
        let uni_value = UniDataValue::uni_from(data_value).unwrap();
        let scalar = uni_value.as_scalar().expect("numeric scalar");
        assert_eq!(scalar.expect_numeric(), "7.50");
    }

    #[test]
    fn test_uni_temporal_type_params_roundtrip_through_data_type() {
        let cases = vec![
            (
                UniDataType::Scalar(UniScalar::Time),
                vec![UniDataValue::Scalar(UniScalarValue::from_i64(3))],
                mudu_type::type_family::TypeFamily::Time,
                3u8,
            ),
            (
                UniDataType::Scalar(UniScalar::Timestamp),
                vec![UniDataValue::Scalar(UniScalarValue::from_i64(4))],
                mudu_type::type_family::TypeFamily::Timestamp,
                4u8,
            ),
            (
                UniDataType::Scalar(UniScalar::TimestampTz),
                vec![UniDataValue::Scalar(UniScalarValue::from_i64(2))],
                mudu_type::type_family::TypeFamily::TimestampTz,
                2u8,
            ),
        ];

        for (uni_ty, params, expected_id, expected_precision) in cases {
            let data_type = uni_ty.clone().uni_to_with_params(Some(params)).unwrap();
            assert_eq!(data_type.type_family(), expected_id);
            let actual_precision = match expected_id {
                mudu_type::type_family::TypeFamily::Time => {
                    data_type.expect_time_param().precision()
                }
                mudu_type::type_family::TypeFamily::Timestamp => {
                    data_type.expect_timestamp_param().precision()
                }
                mudu_type::type_family::TypeFamily::TimestampTz => {
                    data_type.expect_timestamptz_param().precision()
                }
                _ => unreachable!(),
            };
            assert_eq!(actual_precision, expected_precision);

            let uni_back = UniDataType::uni_from(data_type).unwrap();
            assert_eq!(
                serialize_to_json(&uni_back).unwrap(),
                serialize_to_json(&uni_ty).unwrap()
            );
        }
    }

    #[test]
    fn test_uni_temporal_values_roundtrip_through_data_value() {
        let cases = vec![
            (
                UniDataValue::Scalar(UniScalarValue::from_date("2026-05-20".to_string())),
                "2026-05-20",
            ),
            (
                UniDataValue::Scalar(UniScalarValue::from_time("12:34:56.123456".to_string())),
                "12:34:56.123456",
            ),
            (
                UniDataValue::Scalar(UniScalarValue::from_timestamp(
                    "2026-05-20 14:30:45.123456".to_string(),
                )),
                "2026-05-20 14:30:45.123456",
            ),
            (
                UniDataValue::Scalar(UniScalarValue::from_timestamptz(
                    "2026-05-20T14:30:45.123456+08:00".to_string(),
                )),
                "2026-05-20 06:30:45.123456+00:00",
            ),
        ];

        for (uni_value, expected_text) in cases {
            let data_value = uni_value.clone().uni_to().unwrap();
            let uni_back = UniDataValue::uni_from(data_value).unwrap();
            let scalar = uni_back.as_scalar().expect("temporal scalar");
            let text = match scalar {
                UniScalarValue::Date(v)
                | UniScalarValue::Time(v)
                | UniScalarValue::Timestamp(v)
                | UniScalarValue::TimestampTz(v) => v.as_str(),
                _ => unreachable!(),
            };
            assert_eq!(text, expected_text);
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
            UniDataValue::Scalar(UniScalarValue::from_timestamptz(
                "2026-05-20 14:30:45".to_string(),
            )),
        ];
        for value in invalid_values {
            assert!(value.uni_to().is_err());
        }
    }
}
