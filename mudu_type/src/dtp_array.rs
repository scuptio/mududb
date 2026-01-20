use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dt_param::{DTPDyn, DTPStatic};
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DTPArray {
    dat_type: DatType,
    max_size: Option<u32>,
}

impl DTPArray {
    pub fn new(dat_type: DatType) -> DTPArray {
        Self {
            dat_type,
            max_size: None,
        }
    }

    pub fn dat_type(&self) -> &DatType {
        &self.dat_type
    }

    pub fn into_dat_type(self) -> DatType {
        self.dat_type
    }
}
impl Default for DTPArray {
    fn default() -> Self {
        Self {
            dat_type: DatType::default_for(DatTypeID::I32),
            max_size: None,
        }
    }
}

impl DTPDyn for DTPArray {
    fn clone_boxed(&self) -> Box<dyn DTPDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s = utils::json::from_json_str::<Self>(json)?;
        self.dat_type = s.dat_type;
        self.max_size = s.max_size;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        format!("array<{}>", self.dat_type.name())
    }
}

impl Order for DTPArray {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        self.dat_type.cmp_ord(&other.dat_type)
    }
}

impl DTPStatic for DTPArray {}
