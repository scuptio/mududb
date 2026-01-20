use crate::tuple::datum_vec::datum_vec_to_value_vec;
use crate::tuple::tuple_datum::TupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

#[derive(Debug)]
pub struct ProcedureParam {
    xid: XID,
    procedure_id: u64,
    param_list: Vec<DatValue>,
}

impl ProcedureParam {
    pub fn into(self) -> (XID, u64, Vec<DatValue>) {
        (self.xid, self.procedure_id, self.param_list)
    }

    pub fn from_datum_vec(xid: XID, argv: &[&dyn DatumDyn], desc: &TupleFieldDesc) -> RS<Self> {
        let vec = datum_vec_to_value_vec(argv, desc.fields())?;
        Ok(Self::new(xid, 0, vec))
    }

    pub fn from_tuple<T: TupleDatum>(xid: XID, tuple: T, desc: &TupleFieldDesc) -> RS<Self> {
        let vec = tuple.to_value(desc.fields())?;
        Ok(Self::new(xid, 0, vec))
    }
    
    pub fn new(xid: XID, procedure_id: u64, param: Vec<DatValue>) -> ProcedureParam {
        Self { xid, procedure_id, param_list: param }
    }

    pub fn param_list(&self) -> &Vec<DatValue> {
        &self.param_list
    }

    pub fn session_id(&self) -> XID {
        self.xid
    }

    pub fn set_session_id(&mut self, session_id: XID) {
        self.xid = session_id
    }
    pub fn procedure_id(&self) -> u64 {
        self.procedure_id
    }
}
