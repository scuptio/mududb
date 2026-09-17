use crate::data_type::DataType;
use crate::data_type_param::{DTPStatic, DataTypeParamDyn};
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DataTypeParamRecord {
    name: String,
    // field name and its data kind
    field: Vec<(String, DataType)>,
}

impl DataTypeParamDyn for DataTypeParamRecord {
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn> {
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

impl DataTypeParamRecord {
    pub fn new(name: String, field: Vec<(String, DataType)>) -> DataTypeParamRecord {
        let mut map = HashMap::new();
        for (name, ty) in &field {
            map.insert(name.clone(), ty.clone());
        }
        Self { name, field }
    }

    pub fn record_name(&self) -> &String {
        &self.name
    }

    pub fn fields(&self) -> &Vec<(String, DataType)> {
        &self.field
    }

    pub fn into(self) -> (String, Vec<(String, DataType)>) {
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

impl Order for DataTypeParamRecord {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.compare(other))
    }
}

impl DTPStatic for DataTypeParamRecord {}
