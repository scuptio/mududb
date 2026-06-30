//! `tuple::tuple_value` module.
#![allow(missing_docs)]

use mudu_type::dat_value::DatValue;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleValue {
    value: Vec<DatValue>,
}

impl TupleValue {
    pub fn from(value: Vec<DatValue>) -> TupleValue {
        Self { value }
    }

    pub fn values(&self) -> &[DatValue] {
        &self.value
    }

    pub fn into(self) -> Vec<DatValue> {
        self.value
    }
}

impl AsRef<TupleValue> for TupleValue {
    fn as_ref(&self) -> &TupleValue {
        self
    }
}
