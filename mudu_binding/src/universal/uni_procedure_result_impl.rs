use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_procedure_result::UniProcedureResult;
use mudu::common::result::RS;
use mudu_contract::procedure::procedure_result::ProcedureResult;

impl UniProcedureResult {
    pub fn uni_to(self) -> RS<ProcedureResult> {
        let mut vec = Vec::with_capacity(self.return_list.len());
        for mu_d in self.return_list {
            let mu_v = mu_d.uni_to()?;
            vec.push(mu_v);
        }
        Ok(ProcedureResult::new(vec))
    }

    pub fn uni_from(r: ProcedureResult) -> RS<Self> {
        let mut return_list = Vec::with_capacity(r.return_list().len());
        for d in r.into() {
            let mu_v = UniDatValue::uni_from(d)?;
            return_list.push(mu_v);
        }
        Ok(Self { return_list })
    }
}
