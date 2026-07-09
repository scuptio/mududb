use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_procedure_param::UniProcedureParam;
use mudu::common::result::RS;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_type::data_value::DataValue;

impl UniProcedureParam {
    pub fn uni_to(self) -> RS<ProcedureParam> {
        let id = self.session.to_oid();
        let mut vec: Vec<DataValue> = Vec::with_capacity(self.param_list.len());
        for p in self.param_list {
            vec.push(p.uni_to()?);
        }
        let pp = ProcedureParam::new(id, self.procedure, vec);
        Ok(pp)
    }

    pub fn uni_from(p: ProcedureParam) -> RS<Self> {
        let (oid, procedure, param_list) = p.into();
        let mut vec = Vec::with_capacity(param_list.len());
        for v in param_list {
            let uni_val = UniDataValue::uni_from(v)?;
            vec.push(uni_val);
        }
        let mu_p = Self {
            procedure,
            session: UniOid::from_oid(oid),
            param_list: vec,
        };
        Ok(mu_p)
    }
}
