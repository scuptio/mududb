//! Result values returned by a procedure invocation.

use crate::tuple::tuple_datum::TupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu_type::data_value::DataValue;

/// Result values of a procedure invocation.
#[derive(Debug, Clone)]
pub struct ProcedureResult {
    return_list: Vec<DataValue>,
}

impl ProcedureResult {
    /// Consumes `self` and returns the result values.
    pub fn into(self) -> Vec<DataValue> {
        self.return_list
    }

    /// Builds a procedure result from a tuple converted by `desc`.
    pub fn from<T: TupleDatum>(result_tuple: RS<T>, desc: &TupleFieldDesc) -> RS<Self> {
        match result_tuple {
            Ok(t) => {
                let vec = t.to_value(desc.fields())?;
                Ok(Self { return_list: vec })
            }
            Err(e) => Err(e),
        }
    }

    /// Converts the result values into a tuple typed by `desc`.
    pub fn to<T: TupleDatum>(&self, desc: &TupleFieldDesc) -> RS<T> {
        let t = T::from_value(self.return_list(), desc.fields())?;
        Ok(t)
    }

    /// Creates a procedure result from a list of result values.
    pub fn new(return_list: Vec<DataValue>) -> ProcedureResult {
        Self { return_list }
    }

    /// Returns the result values.
    pub fn return_list(&self) -> &Vec<DataValue> {
        &self.return_list
    }
}
