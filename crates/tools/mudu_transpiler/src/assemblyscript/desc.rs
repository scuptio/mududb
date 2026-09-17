//! Generate procedure descriptors from parsed AssemblyScript procedures.

use crate::assemblyscript::procedure::AsProcedure;
use crate::common::desc::{ProcDescField, ProcDescModel};
use mudu_contract::procedure::proc_desc::ProcDesc;

/// Build a [`ProcDesc`] for every procedure in `procedures`.
pub fn gen_procedure_desc_list(module_name: &str, procedures: &[AsProcedure]) -> Vec<ProcDesc> {
    let models = procedures.iter().map(proc_desc_model).collect::<Vec<_>>();
    crate::common::desc::gen_procedure_desc_list(module_name, &models)
}

fn proc_desc_model(procedure: &AsProcedure) -> ProcDescModel {
    ProcDescModel {
        name: procedure.name.clone(),
        // The leading Oid parameter is bound from `UniProcedureParam.session`
        // and never travels in the wire `param_list`, so it is excluded from
        // the desc fields.
        argv_fields: procedure
            .params
            .iter()
            .skip(1)
            .map(|param| ProcDescField {
                name: param.name.clone(),
                data_type: param.value_type.data_type(),
                // A `T | null` option parameter carries the inner data type
                // plus the nullable flag (`DatumDesc::new_nullable`).
                nullable: param.value_type.is_nullable(),
            })
            .collect(),
        result_fields: vec![ProcDescField {
            name: "0".to_string(),
            data_type: procedure.return_value_type.data_type(),
            nullable: false,
        }],
    }
}
