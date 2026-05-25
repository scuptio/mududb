use crate::dat_type_id::DatTypeID;
use crate::dt_param::{DTPDyn, DTPStatic};
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub const TEMPORAL_MAX_PRECISION: u8 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DTPTime {
    precision: u8,
}

impl Default for DTPTime {
    fn default() -> Self {
        Self { precision: 6 }
    }
}

impl DTPTime {
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

impl Order for DTPTime {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.precision.cmp(&other.precision))
    }
}

impl DTPDyn for DTPTime {
    fn clone_boxed(&self) -> Box<dyn DTPDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s: DTPTime = utils::json::from_json_str::<Self>(json)?;
        *self = s;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        format!("{}({})", DatTypeID::Time.name(), self.precision)
    }
}

impl DTPStatic for DTPTime {}
