use crate::data_type::DataType;
use crate::data_type_param::{DTPStatic, DataTypeParamDyn};
use crate::type_family::TypeFamily;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTypeParamArray {
    data_type: DataType,
    max_size: Option<u32>,
}

impl DataTypeParamArray {
    pub fn new(data_type: DataType) -> DataTypeParamArray {
        Self {
            data_type,
            max_size: None,
        }
    }

    pub fn data_type(&self) -> &DataType {
        &self.data_type
    }

    pub fn into_data_type(self) -> DataType {
        self.data_type
    }
}
impl Default for DataTypeParamArray {
    fn default() -> Self {
        Self {
            data_type: DataType::default_for(TypeFamily::I32),
            max_size: None,
        }
    }
}

impl DataTypeParamDyn for DataTypeParamArray {
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s = utils::json::from_json_str::<Self>(json)?;
        self.data_type = s.data_type;
        self.max_size = s.max_size;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        format!("array<{}>", self.data_type.name())
    }
}

impl Order for DataTypeParamArray {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        self.data_type.cmp_ord(&other.data_type)
    }
}

impl DTPStatic for DataTypeParamArray {}
