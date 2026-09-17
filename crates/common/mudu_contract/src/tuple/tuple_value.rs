//! `tuple::tuple_value` module.
#![allow(missing_docs)]

use mudu_type::data_value::DataValue;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleValue {
    value: Vec<DataValue>,
}

impl TupleValue {
    pub fn from(value: Vec<DataValue>) -> TupleValue {
        Self { value }
    }

    pub fn values(&self) -> &[DataValue] {
        &self.value
    }

    pub fn into(self) -> Vec<DataValue> {
        self.value
    }
}

impl AsRef<TupleValue> for TupleValue {
    fn as_ref(&self) -> &TupleValue {
        self
    }
}
