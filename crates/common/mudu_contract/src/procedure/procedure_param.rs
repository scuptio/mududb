//! Parameters passed to a procedure invocation.

use crate::tuple::datum_vec::datum_vec_to_value_vec;
use crate::tuple::tuple_datum::TupleDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;

/// Parameters of a procedure invocation, bound to a session and procedure.
#[derive(Debug)]
pub struct ProcedureParam {
    xid: OID,
    procedure_id: u64,
    param_list: Vec<DataValue>,
}

impl ProcedureParam {
    /// Consumes `self` and returns the session id, procedure id, and parameter values.
    pub fn into(self) -> (OID, u64, Vec<DataValue>) {
        (self.xid, self.procedure_id, self.param_list)
    }

    /// Builds procedure parameters from a slice of datum references converted by `desc`.
    pub fn from_datum_vec(xid: OID, argv: &[&dyn DatumDyn], desc: &TupleFieldDesc) -> RS<Self> {
        let vec = datum_vec_to_value_vec(argv, desc.fields())?;
        Ok(Self::new(xid, 0, vec))
    }

    /// Builds procedure parameters from a tuple converted by `desc`.
    pub fn from_tuple<T: TupleDatum>(xid: OID, tuple: T, desc: &TupleFieldDesc) -> RS<Self> {
        let vec = tuple.to_value(desc.fields())?;
        Ok(Self::new(xid, 0, vec))
    }

    /// Creates procedure parameters from a session id, procedure id, and parameter values.
    pub fn new(xid: OID, procedure_id: u64, param: Vec<DataValue>) -> ProcedureParam {
        Self {
            xid,
            procedure_id,
            param_list: param,
        }
    }

    /// Returns the parameter values.
    pub fn param_list(&self) -> &Vec<DataValue> {
        &self.param_list
    }

    /// Returns the session id the invocation belongs to.
    pub fn session_id(&self) -> OID {
        self.xid
    }

    /// Sets the session id the invocation belongs to.
    pub fn set_session_id(&mut self, session_id: OID) {
        self.xid = session_id
    }

    /// Returns the invoked procedure id.
    pub fn procedure_id(&self) -> u64 {
        self.procedure_id
    }
}
