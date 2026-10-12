//! A tuple of per-field memory values.

use mudu_type::data_value::DataValue;
use serde::{Deserialize, Serialize};

/// A tuple carried as per-field memory values.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleValue {
    value: Vec<DataValue>,
}

impl TupleValue {
    /// Creates a tuple from per-field memory values.
    pub fn from(value: Vec<DataValue>) -> TupleValue {
        Self { value }
    }

    /// Returns the per-field memory values.
    pub fn values(&self) -> &[DataValue] {
        &self.value
    }

    /// Consumes `self` and returns the per-field memory values.
    pub fn into(self) -> Vec<DataValue> {
        self.value
    }
}

impl AsRef<TupleValue> for TupleValue {
    fn as_ref(&self) -> &TupleValue {
        self
    }
}
