//! `database::sql_param_value` module.
#![allow(missing_docs)]

use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;

use crate::database::sql_params::SQLParams;

#[derive(Debug)]
pub struct SQLParamValue {
    param: Vec<DataValue>,
    param_names: Option<Vec<String>>,
}

impl SQLParams for SQLParamValue {
    fn size(&self) -> u64 {
        self.param.len() as u64
    }

    fn get_idx(&self, n: u64) -> Option<&dyn DatumDyn> {
        let data_value = self.param.get(n as usize)?;
        Some(data_value as _)
    }

    fn param_names(&self) -> Option<&[String]> {
        self.param_names.as_deref()
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
        Self {
            param: vec,
            param_names: None,
        }
    }

    /// A parameter set carrying the `param-names` wire field: `names` are
    /// the named (`:name`) placeholders in declaration order, parallel to
    /// `vec`.
    pub fn from_vec_named(vec: Vec<DataValue>, names: Vec<String>) -> Self {
        Self {
            param: vec,
            param_names: Some(names),
        }
    }

    /// Attach (or replace) the parameter names.
    pub fn with_param_names(mut self, names: Option<Vec<String>>) -> Self {
        self.param_names = names;
        self
    }
}
