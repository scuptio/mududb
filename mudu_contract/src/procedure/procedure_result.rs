use crate::tuple::tuple_datum::TupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu_type::dat_value::DatValue;

#[derive(Debug, Clone)]
pub struct ProcedureResult {
    return_list:Vec<DatValue>,
}

impl ProcedureResult {
    pub fn into(self) -> Vec<DatValue> {
        self.return_list
    }
    
    pub fn from<T: TupleDatum>(result_tuple: RS<T>, desc: &TupleFieldDesc) -> RS<Self> {
        match result_tuple {
            Ok(t) => {
                let vec = t.to_value(desc.fields())?;
                Ok(Self {
                    return_list:vec
                })
            }
            Err(e) => {
                Err(e)
            }
        }
    }

    pub fn to<T: TupleDatum>(&self, desc: &TupleFieldDesc) -> RS<T> {
        let t = T::from_value(self.return_list(), desc.fields())?;
        Ok(t)
    }

    pub fn new(return_list:Vec<DatValue>) -> ProcedureResult {
        Self { return_list }
    }

    pub fn return_list(&self) -> &Vec<DatValue> {
        &self.return_list
    }
}