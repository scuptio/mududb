use crate::common::result::RS;
use crate::error::ec::EC;
use crate::error::ec::EC::ConvertErr;
use crate::m_error;
use crate::tuple::rs_tuple_datum::RsTupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ProcResult {
    error: EC,
    result: Vec<Vec<u8>>,
}

impl ProcResult {
    pub fn from<T: RsTupleDatum>(result_tuple: RS<T>, desc: &TupleFieldDesc) -> RS<Self> {
        let (error, result) = match result_tuple {
            Ok(t) => {
                let vec = t.to_binary(desc.fields())?;
                (EC::Ok, vec)
            }
            Err(e) => (e.ec(), vec![]),
        };
        Ok(Self { error, result })
    }

    pub fn to<T: RsTupleDatum>(&self, desc: &TupleFieldDesc) -> RS<RS<T>> {
        match self.error {
            EC::Ok => {
                let r = T::from_binary(&self.result, desc.fields())?;
                Ok(Ok(r))
            }
            _ => Ok(Err(m_error!(self.error.clone()))),
        }
    }

    pub fn to_string(&self, desc: &TupleFieldDesc) -> RS<RS<Vec<String>>> {
        match self.error {
            EC::Ok => {
                let mut vec = vec![];
                for (i, s) in self.result.iter().enumerate() {
                    let datum_desc = &desc.fields()[i];
                    let id = datum_desc.dat_type_id();
                    let internal = id.fn_recv()(s, datum_desc.param_obj())
                        .map_err(|e| m_error!(ConvertErr, "", e))?;
                    let printable = id.fn_output()(&internal, datum_desc.param_obj())
                        .map_err(|e| m_error!(ConvertErr, "", e))?;
                    vec.push(printable.into())
                }
                Ok(Ok(vec))
            }
            _ => Ok(Err(m_error!(self.error.clone()))),
        }
    }

    pub fn new(result: RS<Vec<Vec<u8>>>) -> ProcResult {
        let (error, result) = match result {
            Ok(vec) => (EC::Ok, vec),
            Err(e) => (e.ec(), vec![]),
        };
        Self { error, result }
    }
}
