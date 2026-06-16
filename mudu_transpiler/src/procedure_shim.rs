use askama::Template;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::case_convert::to_kebab_case;
use mudu_contract::procedure::proc;

#[derive(Debug, Clone)]
pub struct ProcedureShimConfig {
    pub generated_by: String,
    pub import_module_name: String,
    pub import_world_name: String,
    pub helper_prefix: String,
}

#[derive(Debug, Clone)]
pub struct ProcedureShim {
    pub name: String,
    pub interface_name: String,
    pub wit_interface_name: String,
    pub adapter_name: String,
    pub wit_adapter_name: String,
    pub fn_exported_name: String,
    pub fn_inner_name: String,
    pub fn_argv_desc: String,
    pub fn_result_desc: String,
    pub fn_proc_desc: String,
    pub wit_fn_exported_name: String,
    pub export_mod_name: String,
    pub guest_struct_name: String,
    pub argv_fields: Vec<ProcedureShimField>,
    pub result_fields: Vec<ProcedureShimField>,
}

#[derive(Debug, Clone)]
pub struct ProcedureShimInput {
    pub name: String,
    pub argv_fields: Vec<ProcedureShimField>,
    pub result_fields: Vec<ProcedureShimField>,
}

#[derive(Debug, Clone)]
pub struct ProcedureShimField {
    pub name: String,
    pub dat_type_expr: String,
}

#[derive(Template)]
#[template(path = "procedure_shim/rust_p2_wrapper.rs.jinja", escape = "none")]
struct RustP2WrapperTemplate {
    procedures: Vec<ProcedureShim>,
    package_name: String,
    config: ProcedureShimConfig,
}

pub fn render_rust_p2_wrapper(
    procedure_inputs: impl IntoIterator<Item = ProcedureShimInput>,
    package_name: &str,
    config: ProcedureShimConfig,
) -> RS<String> {
    let procedures = procedure_inputs
        .into_iter()
        .map(|input| {
            let fn_exported_name = format!("{}{}", proc::MUDU_PROC_P2_PREFIX, input.name);
            let wit_fn_exported_name = to_kebab_case(&fn_exported_name);
            let wit_interface_name = procedure_interface_name(&input.name);
            let wit_adapter_name = procedure_adapter_name(&input.name);
            ProcedureShim {
                name: input.name.clone(),
                interface_name: wit_interface_name.replace('-', "_"),
                wit_interface_name,
                adapter_name: wit_adapter_name.replace('-', "_"),
                wit_adapter_name,
                fn_exported_name,
                fn_inner_name: format!("{}{}", proc::MUDU_PROC_INNER_PREFIX_P2, input.name),
                fn_argv_desc: format!("{}{}", proc::MUDU_PROC_ARGV_DESC_PREFIX, input.name),
                fn_result_desc: format!("{}{}", proc::MUDU_PROC_RESULT_DESC_PREFIX, input.name),
                fn_proc_desc: format!("{}{}", proc::MUDU_PROC_PROC_DESC_PREFIX, input.name),
                wit_fn_exported_name,
                export_mod_name: format!("{}{}", proc::MUDU_PROC_PREFIX_MOD, input.name),
                guest_struct_name: format!("Guest{}", input.name),
                argv_fields: input.argv_fields,
                result_fields: input.result_fields,
            }
        })
        .collect();

    RustP2WrapperTemplate {
        procedures,
        package_name: package_name.to_string(),
        config,
    }
    .render()
    .map_err(|e| m_error!(EC::EncodeErr, "render procedure shim rust wrapper error", e))
}

pub fn procedure_interface_name(name: &str) -> String {
    format!("procedure-{}", to_kebab_case(name))
}

pub fn procedure_adapter_name(name: &str) -> String {
    format!("adapter-{}", to_kebab_case(name))
}
