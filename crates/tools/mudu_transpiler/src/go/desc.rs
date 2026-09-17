//! Generate procedure descriptors from parsed Go procedures.

use crate::common::desc::{ProcDescField, ProcDescModel};
use crate::go::procedure::GoProcedure;
use mudu::utils::case_convert::to_snake_case;
use mudu_contract::procedure::proc_desc::ProcDesc;

/// Build a [`ProcDesc`] for every procedure in `procedures`.
pub fn gen_procedure_desc_list(module_name: &str, procedures: &[GoProcedure]) -> Vec<ProcDesc> {
    let models = procedures.iter().map(proc_desc_model).collect::<Vec<_>>();
    crate::common::desc::gen_procedure_desc_list(module_name, &models)
}

fn proc_desc_model(procedure: &GoProcedure) -> ProcDescModel {
    ProcDescModel {
        name: procedure.name.clone(),
        // The leading muduOid session parameter is bound from
        // `UniProcedureParam.session` and never travels in the wire
        // `param_list`, so it is excluded from the desc fields (the same
        // convention as the AssemblyScript guest's leading Oid parameter).
        // camelCase parameter names normalize to the snake_case wire form.
        // A `*T` option parameter carries the inner data type plus the
        // nullable flag (`DatumDesc::new_nullable`).
        argv_fields: procedure
            .params
            .iter()
            .skip(1)
            .map(|param| ProcDescField {
                name: to_snake_case(&param.name),
                data_type: param.value_type.data_type(),
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
