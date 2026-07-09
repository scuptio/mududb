//! Generate procedure descriptors and shim inputs from parsed AssemblyScript
//! procedures.

use crate::assemblyscript::procedure::{AsProcedure, AsValueType};
use crate::procedure_shim::{ProcedureShimField, ProcedureShimInput};
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

/// Build a [`ProcDesc`] for every procedure in `procedures`.
pub fn gen_procedure_desc_list(module_name: &str, procedures: &[AsProcedure]) -> Vec<ProcDesc> {
    procedures
        .iter()
        .map(|procedure| gen_procedure_desc(module_name, &ProcedureDescModel::from(procedure)))
        .collect()
}

/// Build [`ProcedureShimInput`] values for every procedure in `procedures`.
pub fn gen_procedure_shim_inputs(procedures: &[AsProcedure]) -> Vec<ProcedureShimInput> {
    procedures
        .iter()
        .map(|procedure| ProcedureShimInput::from(ProcedureDescModel::from(procedure)))
        .collect()
}

#[derive(Debug, Clone)]
struct ProcedureDescModel {
    name: String,
    argv_fields: Vec<ProcedureDescField>,
    result_fields: Vec<ProcedureDescField>,
}

#[derive(Debug, Clone)]
struct ProcedureDescField {
    name: String,
    value_type: AsValueType,
}

impl From<&AsProcedure> for ProcedureDescModel {
    fn from(procedure: &AsProcedure) -> Self {
        Self {
            name: procedure.name.clone(),
            argv_fields: procedure
                .params
                .iter()
                .skip(1)
                .map(|param| ProcedureDescField {
                    name: param.name.clone(),
                    value_type: param.value_type,
                })
                .collect(),
            result_fields: vec![ProcedureDescField {
                name: "0".to_string(),
                value_type: procedure.return_value_type,
            }],
        }
    }
}

impl From<ProcedureDescModel> for ProcedureShimInput {
    fn from(model: ProcedureDescModel) -> Self {
        Self {
            name: model.name,
            argv_fields: model
                .argv_fields
                .iter()
                .map(ProcedureShimField::from)
                .collect(),
            result_fields: model
                .result_fields
                .iter()
                .map(ProcedureShimField::from)
                .collect(),
        }
    }
}

impl From<&ProcedureDescField> for ProcedureShimField {
    fn from(field: &ProcedureDescField) -> Self {
        Self {
            name: field.name.clone(),
            data_type_expr: field.value_type.data_type_expr().to_string(),
        }
    }
}

fn gen_procedure_desc(module_name: &str, procedure: &ProcedureDescModel) -> ProcDesc {
    ProcDesc::new(
        module_name.to_string(),
        procedure.name.clone(),
        TupleFieldDesc::new(
            procedure
                .argv_fields
                .iter()
                .map(field_to_datum_desc)
                .collect(),
        ),
        TupleFieldDesc::new(
            procedure
                .result_fields
                .iter()
                .map(field_to_datum_desc)
                .collect(),
        ),
        false,
    )
}

fn field_to_datum_desc(field: &ProcedureDescField) -> DatumDesc {
    DatumDesc::new(field.name.clone(), field.value_type.data_type())
}
