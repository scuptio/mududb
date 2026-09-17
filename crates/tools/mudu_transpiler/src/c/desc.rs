//! Generate procedure descriptors from parsed C procedures.

use crate::c::procedure::CProcedure;
use crate::common::desc::{ProcDescField, ProcDescModel};
use mudu_contract::procedure::proc_desc::ProcDesc;

/// Build a [`ProcDesc`] for every procedure in `procedures`.
pub fn gen_procedure_desc_list(module_name: &str, procedures: &[CProcedure]) -> Vec<ProcDesc> {
    let models = procedures.iter().map(proc_desc_model).collect::<Vec<_>>();
    crate::common::desc::gen_procedure_desc_list(module_name, &models)
}

fn proc_desc_model(procedure: &CProcedure) -> ProcDescModel {
    ProcDescModel {
        name: procedure.name.clone(),
        // The annotated parameters are exactly the wire parameters: the
        // session OID is implicit in `mudu_proc_param.session` and never
        // appears in the marker annotation. A record parameter carries its
        // registry-resolved DataTypeParamRecord; an `option<T>` parameter
        // carries the inner data type plus the nullable flag
        // (`DatumDesc::new_nullable`).
        argv_fields: procedure
            .params
            .iter()
            .map(|param| ProcDescField {
                name: param.name.clone(),
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
