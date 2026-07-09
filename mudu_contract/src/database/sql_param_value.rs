//! `database::sql_param_value` module.
#![allow(missing_docs)]

use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;

use crate::database::sql_params::SQLParams;

pub struct SQLParamValue {
    param: Vec<DataValue>,
}

impl SQLParams for SQLParamValue {
    fn size(&self) -> u64 {
        self.param.len() as u64
    }

    fn get_idx(&self, n: u64) -> Option<&dyn DatumDyn> {
        let data_value = self.param.get(n as usize)?;
        Some(data_value as _)
    }
}

impl SQLParamValue {
    pub fn into(self) -> Vec<DataValue> {
        self.param
    }

    pub fn params(&self) -> &[DataValue] {
        &self.param
    }
    pub fn from_vec(vec: Vec<DataValue>) -> Self {
        Self { param: vec }
    }
}
