use crate::dat_type_id::DatTypeID;
use crate::dt_param::{DTPDyn, DTPStatic};
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DTPNumeric {
    precision: u8,
    scale: u8,
}

pub const NUMERIC_MAX_PRECISION: u8 = 38;
pub const NUMERIC_MAX_SCALE: u8 = 38;

impl Default for DTPNumeric {
    fn default() -> Self {
        Self {
            precision: 38,
            scale: 0,
        }
    }
}

impl DTPNumeric {
    pub fn new(precision: u8, scale: u8) -> Self {
        Self { precision, scale }
    }

    pub fn precision(&self) -> u8 {
        self.precision
    }

    pub fn scale(&self) -> u8 {
        self.scale
    }

    pub fn compare(&self, other: &Self) -> Ordering {
        self.precision
            .cmp(&other.precision)
            .then(self.scale.cmp(&other.scale))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.precision == 0 {
            return Err("numeric precision must be greater than zero".to_string());
        }
        if self.precision > NUMERIC_MAX_PRECISION {
            return Err(format!(
                "numeric precision must be less than or equal to {}",
                NUMERIC_MAX_PRECISION
            ));
        }
        if self.scale > NUMERIC_MAX_SCALE {
            return Err(format!(
                "numeric scale must be less than or equal to {}",
                NUMERIC_MAX_SCALE
            ));
        }
        if self.scale > self.precision {
            return Err("numeric scale must be less than or equal to precision".to_string());
        }
        Ok(())
    }
}

impl Order for DTPNumeric {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.compare(other))
    }
}

impl DTPDyn for DTPNumeric {
    fn clone_boxed(&self) -> Box<dyn DTPDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s: DTPNumeric = utils::json::from_json_str::<Self>(json)?;
        *self = s;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        format!(
            "{}({}, {})",
            DatTypeID::Numeric.name(),
            self.precision,
            self.scale
        )
    }
}

impl DTPStatic for DTPNumeric {}
