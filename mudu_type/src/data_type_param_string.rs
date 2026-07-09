use crate::data_type_param::{DTPStatic, DataTypeParamDyn};
use crate::type_family::TypeFamily;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DataTypeParamString {
    length: u32,
}

impl DataTypeParamString {
    pub fn new(length: u32) -> Self {
        Self { length }
    }

    pub fn length(&self) -> u32 {
        self.length
    }

    pub fn compare(&self, other: &Self) -> Ordering {
        match (self.fixed_length(), other.fixed_length()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => Ordering::Equal,
        }
    }

    pub fn fixed_length(&self) -> bool {
        false
    }
}

impl Order for DataTypeParamString {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.compare(other))
    }
}

impl DataTypeParamDyn for DataTypeParamString {
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s: DataTypeParamString = utils::json::from_json_str::<Self>(json)?;
        *self = s;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        TypeFamily::String.name().to_string()
    }
}

impl DTPStatic for DataTypeParamString {}
