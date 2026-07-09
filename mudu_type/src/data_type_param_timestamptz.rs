use crate::data_type_param::{DTPStatic, DataTypeParamDyn};
use crate::data_type_param_time::TEMPORAL_MAX_PRECISION;
use crate::type_family::TypeFamily;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTypeParamTimestampTz {
    precision: u8,
}

impl Default for DataTypeParamTimestampTz {
    fn default() -> Self {
        Self { precision: 6 }
    }
}

impl DataTypeParamTimestampTz {
    pub fn new(precision: u8) -> Self {
        Self { precision }
    }

    pub fn precision(&self) -> u8 {
        self.precision
    }

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

impl Order for DataTypeParamTimestampTz {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.precision.cmp(&other.precision))
    }
}

impl DataTypeParamDyn for DataTypeParamTimestampTz {
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s: DataTypeParamTimestampTz = utils::json::from_json_str::<Self>(json)?;
        *self = s;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        format!("{}({})", TypeFamily::TimestampTz.name(), self.precision)
    }
}

impl DTPStatic for DataTypeParamTimestampTz {}
