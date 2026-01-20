use crate::dat_type::DatType;
use crate::dt_param::{DTPDyn, DTPStatic};
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DTPRecord {
    name: String,
    // field name and its data kind
    field: Vec<(String, DatType)>,
}

impl DTPDyn for DTPRecord {
    fn clone_boxed(&self) -> Box<dyn DTPDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s: Self = utils::json::from_json_str::<Self>(json)?;
        self.name = s.name;
        self.field = s.field;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        self.name.clone()
    }
}

impl DTPRecord {
    pub fn new(name: String, field: Vec<(String, DatType)>) -> DTPRecord {
        let mut map = HashMap::new();
        for (name, ty) in &field {
            map.insert(name.clone(), ty.clone());
        }
        Self { name, field }
    }

    pub fn record_name(&self) -> &String {
        &self.name
    }

    pub fn fields(&self) -> &Vec<(String, DatType)> {
        &self.field
    }

    pub fn into(self) -> (String, Vec<(String, DatType)>) {
        (self.name, self.field)
    }

    fn compare(&self, other: &Self) -> Ordering {
        if self.name.eq(&other.name) {
            Ordering::Equal
        } else {
            self.field.len().cmp(&other.field.len())
        }
    }
}

impl Order for DTPRecord {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.compare(other))
    }
}

impl DTPStatic for DTPRecord {}
