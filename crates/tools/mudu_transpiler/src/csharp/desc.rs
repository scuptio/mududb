//! Generate procedure descriptors from parsed C# procedures.

use crate::common::desc::{ProcDescField, ProcDescModel};
use crate::csharp::procedure::CsProcedure;
use mudu::utils::case_convert::to_snake_case;
use mudu_contract::procedure::proc_desc::ProcDesc;

/// Build a [`ProcDesc`] for every procedure in `procedures`.
pub fn gen_procedure_desc_list(module_name: &str, procedures: &[CsProcedure]) -> Vec<ProcDesc> {
    let models = procedures.iter().map(proc_desc_model).collect::<Vec<_>>();
    crate::common::desc::gen_procedure_desc_list(module_name, &models)
}

fn proc_desc_model(procedure: &CsProcedure) -> ProcDescModel {
    ProcDescModel {
        name: procedure.name.clone(),
        // The leading MuduOid session parameter is bound from
        // `UniProcedureParam.session` and never travels in the wire
        // `param_list`, so it is excluded from the desc fields (the same
        // convention as the AssemblyScript guest's leading Oid parameter).
        // camelCase parameter names normalize to the snake_case wire form.
        argv_fields: procedure
            .params
            .iter()
            .skip(1)
            .map(|param| ProcDescField {
                name: to_snake_case(&param.name),
                data_type: param.value_type.data_type(),
                nullable: false,
            })
            .collect(),
        result_fields: vec![ProcDescField {
            name: "0".to_string(),
            data_type: procedure.return_value_type.data_type(),
            nullable: false,
        }],
    }
}
