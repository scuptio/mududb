use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;

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
