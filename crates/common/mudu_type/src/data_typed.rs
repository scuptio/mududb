use crate::data_type::DataType;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;

/// A value paired with its data type.
#[derive(Clone, Debug)]
pub struct DataTyped {
    data_type: DataType,
    data_internal: DataValue,
}

impl DataTyped {
    /// Creates a typed value holding an `i32`.
    pub fn from_i32(val: i32) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::I32),
            DataValue::from_i32(val),
        )
    }

    /// Creates a typed value holding an `i64`.
    pub fn from_i64(val: i64) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::I64),
            DataValue::from_i64(val),
        )
    }

    /// Creates a typed value holding an `i128`.
    pub fn from_i128(val: i128) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::I128),
            DataValue::from_i128(val),
        )
    }

    /// Creates a typed value holding an OID (`u128`).
    pub fn from_oid(val: u128) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::U128),
            DataValue::from_u128(val),
        )
    }

    /// Creates a typed value holding an `f32`.
    pub fn from_f32(val: f32) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::F32),
            DataValue::from_f32(val),
        )
    }

    /// Creates a typed value holding an `f64`.
    pub fn from_f64(val: f64) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::F64),
            DataValue::from_f64(val),
        )
    }

    /// Creates a typed value holding a string.
    pub fn from_string(val: String) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::String),
            DataValue::from_string(val),
        )
    }

    /// Creates a typed value holding a numeric.
    pub fn from_numeric(val: Numeric) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Numeric),
            DataValue::from_numeric(val),
        )
    }

    /// Creates a typed value holding a date.
    pub fn from_date(val: DateValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Date),
            DataValue::from_date(val),
        )
    }

    /// Creates a typed value holding a time.
    pub fn from_time(val: TimeValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Time),
            DataValue::from_time(val),
        )
    }

    /// Creates a typed value holding a timestamp.
    pub fn from_timestamp(val: TimestampValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::Timestamp),
            DataValue::from_timestamp(val),
        )
    }

    /// Creates a typed value holding a timestamp with time zone.
    pub fn from_timestamptz(val: TimestampTzValue) -> Self {
        Self::new(
            DataType::default_for(TypeFamily::TimestampTz),
            DataValue::from_timestamptz(val),
        )
    }

    /// Creates a typed value from an explicit data type and value.
    pub fn new(data_type: DataType, data_internal: DataValue) -> Self {
        Self {
            data_type,
            data_internal,
        }
    }

    /// Returns the data type of the value.
    pub fn data_type(&self) -> &DataType {
        &self.data_type
    }

    /// Returns the untyped value.
    pub fn data_internal(&self) -> &DataValue {
        &self.data_internal
    }
}
