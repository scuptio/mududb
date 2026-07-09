use crate::data_type::DataType;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;

#[derive(Clone, Debug)]
pub struct DataTyped {
    data_type: DataType,
    data_internal: DataValue,
}

impl DataTyped {
    pub fn from_i32(val: i32) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::I32),
            DataValue::from_i32(val),
        )
    }

    pub fn from_i64(val: i64) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::I64),
            DataValue::from_i64(val),
        )
    }

    pub fn from_i128(val: i128) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::I128),
            DataValue::from_i128(val),
        )
    }

    pub fn from_oid(val: u128) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::U128),
            DataValue::from_u128(val),
        )
    }

    pub fn from_f32(val: f32) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::F32),
            DataValue::from_f32(val),
        )
    }

    pub fn from_f64(val: f64) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::F64),
            DataValue::from_f64(val),
        )
    }

    pub fn from_string(val: String) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::String),
            DataValue::from_string(val),
        )
    }

    pub fn from_numeric(val: Numeric) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Numeric),
            DataValue::from_numeric(val),
        )
    }

    pub fn from_date(val: DateValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Date),
            DataValue::from_date(val),
        )
    }

    pub fn from_time(val: TimeValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Time),
            DataValue::from_time(val),
        )
    }

    pub fn from_timestamp(val: TimestampValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Timestamp),
            DataValue::from_timestamp(val),
        )
    }

    pub fn from_timestamptz(val: TimestampTzValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::TimestampTz),
            DataValue::from_timestamptz(val),
        )
    }

    pub fn new(data_type: DataType, data_internal: DataValue) -> Self {
        Self {
            data_type,
            data_internal,
        }
    }

    pub fn data_type(&self) -> &DataType {
        &self.data_type
    }

    pub fn data_internal(&self) -> &DataValue {
        &self.data_internal
    }
}
