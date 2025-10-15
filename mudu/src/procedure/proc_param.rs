use crate::common::result::RS;
use crate::common::xid::XID;
use crate::tuple::datum::DatumDyn;
use crate::tuple::datum_vec::datum_vec_to_bin_vec;
use crate::tuple::rs_tuple_datum::RsTupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use serde::{Deserialize, Serialize};

impl ProcParam {
    pub fn from_datum_vec(xid: XID, argv: &[&dyn DatumDyn], desc: &TupleFieldDesc) -> RS<Self> {
        let vec = datum_vec_to_bin_vec(argv, desc.fields())?;
        Ok(Self::new(xid, vec))
    }

    pub fn from_tuple<T: RsTupleDatum>(xid: XID, tuple: T, desc: &TupleFieldDesc) -> RS<Self> {
        let vec = tuple.to_binary(desc.fields())?;
        Ok(Self::new(xid, vec))
    }

    pub fn new(xid: XID, param: Vec<Vec<u8>>) -> ProcParam {
        Self { xid, param }
    }

    pub fn param_vec(&self) -> &Vec<Vec<u8>> {
        &self.param
    }

    pub fn into_param_vec(self) -> Vec<Vec<u8>> {
        self.param
    }

    pub fn xid(&self) -> XID {
        self.xid
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProcParam {
    xid: XID,
    param: Vec<Vec<u8>>,
}


#[cfg(test)]
pub mod test {
    use crate::common::serde_utils::{deserialize_sized_from, serialize_sized_to_vec};
    use crate::procedure::proc_param::ProcParam;

    #[test]
    fn test() {
        let param = ProcParam::new(333, vec![vec![1, 2], vec![3, 4]]);
        let vec = serialize_sized_to_vec(&param).unwrap();
        let (param2, _): (ProcParam, _) = deserialize_sized_from(&vec).unwrap();
        assert_eq!(param.xid, param2.xid);
        assert_eq!(param.param, param2.param);
    }
}
