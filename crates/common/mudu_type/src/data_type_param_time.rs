use crate::data_type_param::{DTPStatic, DataTypeParamDyn};
use crate::type_family::TypeFamily;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Maximum fractional-second precision of the temporal types.
pub const TEMPORAL_MAX_PRECISION: u8 = 6;

/// The parameter of the `Time` type: fractional-second precision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTypeParamTime {
    precision: u8,
}

impl Default for DataTypeParamTime {
    fn default() -> Self {
        Self { precision: 6 }
    }
}

impl DataTypeParamTime {
    /// Creates a time parameter with the given fractional-second precision.
    pub fn new(precision: u8) -> Self {
        Self { precision }
    }

    /// Returns the fractional-second precision.
    pub fn precision(&self) -> u8 {
        self.precision
    }

    /// Validates the precision against [`TEMPORAL_MAX_PRECISION`]; returns
    /// an error message on failure.
    pub fn validate(&self) -> Result<(), String> {
        if self.precision > TEMPORAL_MAX_PRECISION {
            return Err(format!(
                "temporal precision must be less than or equal to {}",
                TEMPORAL_MAX_PRECISION
            ));
        }
        Ok(())
    }
}

impl Order for DataTypeParamTime {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.precision.cmp(&other.precision))
    }
}

impl DataTypeParamDyn for DataTypeParamTime {
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s: DataTypeParamTime = utils::json::from_json_str::<Self>(json)?;
        *self = s;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        format!("{}({})", TypeFamily::Time.name(), self.precision)
    }
}

impl DTPStatic for DataTypeParamTime {}
