//! Value-conversion continuation block: maps between the universal
//! `UniDataValue` model and rusqlite values, and infers result-set types.

use super::MockSqliteMuduSysCall;
use crate::{
    UniDataType, UniDataValue, UniRecordField, UniRecordType, UniScalar, UniScalarValue,
    UniTupleRow,
};
use rusqlite::types::{Value, ValueRef};

impl MockSqliteMuduSysCall {
    pub(super) fn to_db_values(values: Vec<UniDataValue>) -> Result<Vec<Value>, String> {
        values
            .into_iter()
            .map(|value| match value {
                UniDataValue::Scalar(scalar) => Self::to_db_scalar(scalar),
                UniDataValue::Binary(bytes) => Ok(Value::Blob(bytes)),
                other => Err(format!("unsupported sqlite parameter type: {other:?}")),
            })
            .collect()
    }

    fn to_db_scalar(value: UniScalarValue) -> Result<Value, String> {
        match value {
            UniScalarValue::Bool(v) => Ok(Value::Integer(if v { 1 } else { 0 })),
            UniScalarValue::U8(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I8(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::U16(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I16(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::U32(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I32(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::U64(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I64(v) => Ok(Value::Integer(v)),
            UniScalarValue::F32(v) => Ok(Value::Real(v as f64)),
            UniScalarValue::F64(v) => Ok(Value::Real(v)),
            UniScalarValue::Char(v) => Ok(Value::Text(v.to_string())),
            UniScalarValue::String(v) => Ok(Value::Text(v)),
            UniScalarValue::U128(v) | UniScalarValue::I128(v) | UniScalarValue::Blob(v) => {
                Ok(Value::Blob(v))
            }
            UniScalarValue::Numeric(v)
            | UniScalarValue::Date(v)
            | UniScalarValue::Time(v)
            | UniScalarValue::Timestamp(v)
            | UniScalarValue::TimestampTz(v) => Ok(Value::Text(v)),
            UniScalarValue::Null => Ok(Value::Null),
        }
    }

    pub(super) fn read_row(
        row: &rusqlite::Row<'_>,
        inferred_types: &mut [Option<UniDataType>],
    ) -> Result<UniTupleRow, String> {
        let mut fields = Vec::with_capacity(row.as_ref().column_count());
        for (index, inferred_type) in inferred_types.iter_mut().enumerate() {
            let value = row.get_ref(index).map_err(|error| error.to_string())?;
            let field = Self::to_uni_data_value(value)?;
            if inferred_type.is_none() {
                *inferred_type = Some(Self::infer_uni_data_type(&field));
            }
            fields.push(field);
        }

        Ok(UniTupleRow { fields })
    }

    pub(super) fn build_tuple_desc(
        column_names: Vec<String>,
        inferred_types: Vec<Option<UniDataType>>,
    ) -> UniRecordType {
        let record_fields = column_names
            .into_iter()
            .zip(inferred_types)
            .map(|(field_name, field_type)| UniRecordField {
                field_name,
                field_type: field_type.unwrap_or(UniDataType::Scalar(UniScalar::String)),
                field_attrs: Vec::new(),
            })
            .collect();

        UniRecordType {
            record_name: String::new(),
            record_fields,
        }
    }

    fn to_uni_data_value(value: ValueRef<'_>) -> Result<UniDataValue, String> {
        match value {
            ValueRef::Null => Err("NULL value is not supported".to_string()),
            ValueRef::Integer(v) => Ok(UniDataValue::Scalar(UniScalarValue::I64(v))),
            ValueRef::Real(v) => Ok(UniDataValue::Scalar(UniScalarValue::F64(v))),
            ValueRef::Text(v) => Ok(UniDataValue::Scalar(UniScalarValue::String(
                String::from_utf8_lossy(v).into_owned(),
            ))),
            ValueRef::Blob(v) => Ok(UniDataValue::Binary(v.to_vec())),
        }
    }

    fn infer_uni_data_type(value: &UniDataValue) -> UniDataType {
        match value {
            UniDataValue::Scalar(UniScalarValue::Bool(_)) => UniDataType::Scalar(UniScalar::Bool),
            UniDataValue::Scalar(UniScalarValue::U8(_)) => UniDataType::Scalar(UniScalar::U8),
            UniDataValue::Scalar(UniScalarValue::I8(_)) => UniDataType::Scalar(UniScalar::I8),
            UniDataValue::Scalar(UniScalarValue::U16(_)) => UniDataType::Scalar(UniScalar::U16),
            UniDataValue::Scalar(UniScalarValue::I16(_)) => UniDataType::Scalar(UniScalar::I16),
            UniDataValue::Scalar(UniScalarValue::U32(_)) => UniDataType::Scalar(UniScalar::U32),
            UniDataValue::Scalar(UniScalarValue::I32(_)) => UniDataType::Scalar(UniScalar::I32),
            UniDataValue::Scalar(UniScalarValue::U64(_)) => UniDataType::Scalar(UniScalar::U64),
            UniDataValue::Scalar(UniScalarValue::I64(_)) => UniDataType::Scalar(UniScalar::I64),
            UniDataValue::Scalar(UniScalarValue::F32(_)) => UniDataType::Scalar(UniScalar::F32),
            UniDataValue::Scalar(UniScalarValue::F64(_)) => UniDataType::Scalar(UniScalar::F64),
            UniDataValue::Scalar(UniScalarValue::Char(_)) => UniDataType::Scalar(UniScalar::Char),
            UniDataValue::Scalar(UniScalarValue::String(_)) => {
                UniDataType::Scalar(UniScalar::String)
            }
            UniDataValue::Scalar(UniScalarValue::U128(_)) => UniDataType::Scalar(UniScalar::U128),
            UniDataValue::Scalar(UniScalarValue::I128(_)) => UniDataType::Scalar(UniScalar::I128),
            UniDataValue::Scalar(UniScalarValue::Blob(_)) => UniDataType::Scalar(UniScalar::Blob),
            UniDataValue::Scalar(UniScalarValue::Numeric(_)) => {
                UniDataType::Scalar(UniScalar::Numeric)
            }
            UniDataValue::Scalar(UniScalarValue::Date(_)) => UniDataType::Scalar(UniScalar::Date),
            UniDataValue::Scalar(UniScalarValue::Time(_)) => UniDataType::Scalar(UniScalar::Time),
            UniDataValue::Scalar(UniScalarValue::Timestamp(_)) => {
                UniDataType::Scalar(UniScalar::Timestamp)
            }
            UniDataValue::Scalar(UniScalarValue::TimestampTz(_)) => {
                UniDataType::Scalar(UniScalar::TimestampTz)
            }
            UniDataValue::Binary(_) => UniDataType::Scalar(UniScalar::Blob),
            UniDataValue::Scalar(UniScalarValue::Null)
            | UniDataValue::Array(_)
            | UniDataValue::Record(_) => UniDataType::Scalar(UniScalar::String),
        }
    }
}
