//! Generate procedure descriptors shared by the byte-pipe guest front-ends.

use mudu::common::result::RS;
use mudu::utils::json::to_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::data_type::DataType;
use std::collections::HashMap;

/// Language-neutral procedure signature used to build a [`ProcDesc`].
///
/// Front-ends convert their parsed procedures into this model: the bound
/// session/OID parameter is already excluded from `argv_fields` (it travels
/// in `UniProcedureParam.session`, not in the wire `param_list`).
#[derive(Debug, Clone)]
pub struct ProcDescModel {
    /// Procedure name (snake_case wire name).
    pub name: String,
    /// Wire parameter fields in declared order.
    pub argv_fields: Vec<ProcDescField>,
    /// Result fields; the single-result convention names its field `"0"`.
    pub result_fields: Vec<ProcDescField>,
}

/// One procedure descriptor field: a name plus its Mudu data type.
#[derive(Debug, Clone)]
pub struct ProcDescField {
    /// Field name (parameter name, or the positional result field name).
    pub name: String,
    /// Field data type.
    pub data_type: DataType,
    /// Whether the field accepts a null datum (a WIT `option<T>` parameter).
    pub nullable: bool,
}

/// Build a [`ProcDesc`] for every procedure model in `procedures`.
pub fn gen_procedure_desc_list(module_name: &str, procedures: &[ProcDescModel]) -> Vec<ProcDesc> {
    procedures
        .iter()
        .map(|procedure| gen_procedure_desc(module_name, procedure))
        .collect()
}

/// Write the single-module `ModProcDesc` JSON document every front-end
/// emits for `--package-desc`.
pub fn write_package_desc(
    module_name: &str,
    proc_desc_list: Vec<ProcDesc>,
    desc_file: &str,
) -> RS<()> {
    let modules = HashMap::from_iter(vec![(module_name.to_string(), proc_desc_list)]);
    let package_desc = ModProcDesc::new(modules);
    let json = to_json_str(&package_desc)?;
    mudu_sys::fs::sync::sync_write(desc_file, json)?;
    Ok(())
}

fn gen_procedure_desc(module_name: &str, procedure: &ProcDescModel) -> ProcDesc {
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

fn field_to_datum_desc(field: &ProcDescField) -> DatumDesc {
    DatumDesc::new_nullable(field.name.clone(), field.data_type.clone(), field.nullable)
}
