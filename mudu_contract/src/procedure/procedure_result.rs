//! `procedure::procedure_result` module.
#![allow(missing_docs)]

use crate::tuple::tuple_datum::TupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu_type::data_value::DataValue;

#[derive(Debug, Clone)]
pub struct ProcedureResult {
    return_list: Vec<DataValue>,
}

impl ProcedureResult {
    pub fn into(self) -> Vec<DataValue> {
        self.return_list
    }

    pub fn from<T: TupleDatum>(result_tuple: RS<T>, desc: &TupleFieldDesc) -> RS<Self> {
        match result_tuple {
            Ok(t) => {
                let vec = t.to_value(desc.fields())?;
                Ok(Self { return_list: vec })
            }
            Err(e) => Err(e),
        }
    }

    pub fn to<T: TupleDatum>(&self, desc: &TupleFieldDesc) -> RS<T> {
        let t = T::from_value(self.return_list(), desc.fields())?;
        Ok(t)
    }

    pub fn new(return_list: Vec<DataValue>) -> ProcedureResult {
        Self { return_list }
    }

    pub fn return_list(&self) -> &Vec<DataValue> {
        &self.return_list
    }
}
