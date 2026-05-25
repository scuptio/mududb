use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;

#[derive(Clone, Debug)]
pub struct DatTyped {
    dat_type: DatType,
    dat_internal: DatValue,
}

impl DatTyped {
    pub fn from_i32(val: i32) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::I32),
            DatValue::from_i32(val),
        )
    }

    pub fn from_i64(val: i64) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::I64),
            DatValue::from_i64(val),
        )
    }

    pub fn from_i128(val: i128) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::I128),
            DatValue::from_i128(val),
        )
    }

    pub fn from_oid(val: u128) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::U128),
            DatValue::from_u128(val),
        )
    }

    pub fn from_f32(val: f32) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::F32),
            DatValue::from_f32(val),
        )
    }

    pub fn from_f64(val: f64) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::F64),
            DatValue::from_f64(val),
        )
    }

    pub fn from_string(val: String) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::String),
            DatValue::from_string(val),
        )
    }

    pub fn from_numeric(val: Numeric) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::Numeric),
            DatValue::from_numeric(val),
        )
    }

    pub fn from_date(val: DateValue) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::Date),
            DatValue::from_date(val),
        )
    }

    pub fn from_time(val: TimeValue) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::Time),
            DatValue::from_time(val),
        )
    }

    pub fn from_timestamp(val: TimestampValue) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::Timestamp),
            DatValue::from_timestamp(val),
        )
    }

    pub fn from_timestamptz(val: TimestampTzValue) -> Self {
        Self::new(
            DatType::default_for(DatTypeID::TimestampTz),
            DatValue::from_timestamptz(val),
        )
    }

    pub fn new(dat_type: DatType, dat_internal: DatValue) -> Self {
        Self {
            dat_type,
            dat_internal,
        }
    }

    pub fn dat_type(&self) -> &DatType {
        &self.dat_type
    }

    pub fn dat_internal(&self) -> &DatValue {
        &self.dat_internal
    }
}
