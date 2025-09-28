use crate::common::result::RS;
use crate::error::ec::EC;
use crate::error::ec::EC::ConvertErr;
use crate::m_error;
use crate::tuple::rs_tuple_datum::RsTupleDatum;
use crate::tuple::tuple_item_desc::TupleItemDesc;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ProcResult {
    error: EC,
    result: Vec<Vec<u8>>,
}

impl ProcResult {
    pub fn from<T: RsTupleDatum>(result_tuple: RS<T>, desc: &TupleItemDesc) -> RS<Self> {
        let (error, result) = match result_tuple {
            Ok(t) => {
                let vec = t.to_binary(desc.vec_datum_desc())?;
                (EC::Ok, vec)
            }
            Err(e) => {
                (e.ec(), vec![])
            }
        };
        Ok(Self {
            error,
            result,
        })
    }

    pub fn to<T: RsTupleDatum>(&self, desc: &TupleItemDesc) -> RS<RS<T>> {
        match self.error {
            EC::Ok => {
                let r = T::from_binary(&self.result, desc.vec_datum_desc())?;
                Ok(Ok(r))
            }
            _ => {
                Ok(Err(m_error!(self.error.clone())))
            }
        }
    }

    pub fn to_string(&self, desc: &TupleItemDesc) -> RS<RS<Vec<String>>> {
        match self.error {
            EC::Ok => {
                let mut vec = vec![];
                for (i, s) in self.result.iter().enumerate() {
                    let datum_desc = &desc.vec_datum_desc()[i];
                    let id = datum_desc.dat_type_id();
                    let internal = id.fn_recv()(s, datum_desc.dat_type_param())
                        .map_err(|e| {
                            m_error!(ConvertErr, "", e)
                        })?;
                    let printable = id.fn_output()(&internal, datum_desc.dat_type_param())
                        .map_err(|e| {
                            m_error!(ConvertErr, "", e)
                        })?;
                    vec.push(printable.into())
                }
                Ok(Ok(vec))
            }
            _ => {
                Ok(Err(m_error!(self.error.clone())))
            }
        }
    }

    pub fn new(result: RS<Vec<Vec<u8>>>) -> ProcResult {
        let (error, result) = match result {
            Ok(vec) => (EC::Ok, vec),
            Err(e) => (e.ec(), vec![])
        };
        Self {
            error,
            result,
        }
    }
}