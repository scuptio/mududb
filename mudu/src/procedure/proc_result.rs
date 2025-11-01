use crate::common::result::RS;
use crate::common::serde_utils::{deserialize_sized_from, serialize_sized_to_vec};
use crate::error::ec::EC::TypeBaseErr;
use crate::m_error;
use crate::tuple::rs_tuple_datum::RsTupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ProcResult {
    result: Result<Vec<Vec<u8>>, Vec<u8>>,
}

impl ProcResult {
    pub fn from<T: RsTupleDatum>(result_tuple: RS<T>, desc: &TupleFieldDesc) -> RS<Self> {
        let result = match result_tuple {
            Ok(t) => {
                let vec = t.to_binary(desc.fields())?;
                Ok(vec)
            }
            Err(e) => Err(serialize_sized_to_vec(&e)?),
        };
        Ok(Self { result })
    }

    pub fn to<T: RsTupleDatum>(&self, desc: &TupleFieldDesc) -> RS<RS<T>> {
        match &self.result {
            Ok(vec) => {
                let r = T::from_binary(vec, desc.fields())?;
                Ok(Ok(r))
            }
            Err(bytes) => {
                let (err, _) = deserialize_sized_from(bytes)?;
                Ok(Err(err))
            }
        }
    }

    pub fn to_string(&self, desc: &TupleFieldDesc) -> RS<RS<Vec<String>>> {
        match &self.result {
            Ok(ret_list) => {
                let mut vec: Vec<String> = vec![];
                for (i, s) in ret_list.iter().enumerate() {
                    let datum_desc = &desc.fields()[i];
                    let id = datum_desc.dat_type_id();
                    let internal = id.fn_recv()(s, datum_desc.param_obj())
                        .map_err(|e| m_error!(TypeBaseErr, "", e))?;
                    let printable = id.fn_output()(&internal, datum_desc.param_obj())
                        .map_err(|e| m_error!(TypeBaseErr, "", e))?;
                    vec.push(printable.into())
                }
                Ok(Ok(vec))
            }
            Err(bytes) => {
                let (err, _) = deserialize_sized_from(bytes)?;
                Ok(Err(err))
            }
        }
    }

    pub fn new(result: RS<Vec<Vec<u8>>>) -> ProcResult {
        match result {
            Ok(vec) => ProcResult { result: Ok(vec) },
            Err(e) => ProcResult { result: Err(serialize_sized_to_vec(&e).unwrap()) },
        }
    }
}
