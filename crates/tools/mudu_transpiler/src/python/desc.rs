//! Generate procedure descriptors from parsed Python procedures.

use crate::common::desc::{ProcDescField, ProcDescModel};
use crate::python::procedure::PyProcedure;
use mudu_contract::procedure::proc_desc::ProcDesc;

/// Build a [`ProcDesc`] for every procedure in `procedures`.
pub fn gen_procedure_desc_list(module_name: &str, procedures: &[PyProcedure]) -> Vec<ProcDesc> {
    let models = procedures.iter().map(proc_desc_model).collect::<Vec<_>>();
    crate::common::desc::gen_procedure_desc_list(module_name, &models)
}

fn proc_desc_model(procedure: &PyProcedure) -> ProcDescModel {
    ProcDescModel {
        name: procedure.name.clone(),
        // The session parameter is bound from `UniProcedureParam.session`
        // and never travels in `param_list`, so it is excluded from the
        // desc fields (the AssemblyScript front-end skips its leading Oid
        // parameter the same way). An option hint (`Optional[T]` /
        // `T | None`) carries the inner data type plus the nullable flag
        // (`DatumDesc::new_nullable`).
        argv_fields: procedure
            .params
            .iter()
            .skip(if procedure.session_arg.is_some() {
                1
            } else {
                0
            })
            .map(|param| ProcDescField {
                name: param.name.clone(),
                data_type: param.value_type.data_type(),
                nullable: param.value_type.is_nullable(),
            })
            .collect(),
        result_fields: procedure
            .return_value_types
            .iter()
            .enumerate()
            .map(|(index, value_type)| ProcDescField {
                name: index.to_string(),
                data_type: value_type.data_type(),
                nullable: false,
            })
            .collect(),
    }
}
