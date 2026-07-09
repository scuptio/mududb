//! Render Rust P2 wrapper shims from procedure metadata.
//!
//! The generated wrapper exports a per-procedure `mp2_*` function that
//! deserializes arguments, calls the inner procedure, and serializes the
//! result for the WIT component boundary.

use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_kebab_case;
use mudu_contract::procedure::proc;

/// Configuration values consumed by the Rust P2 wrapper template.
#[derive(Debug, Clone)]
pub struct ProcedureShimConfig {
    /// Generator attribution string.
    pub generated_by: String,
    /// WIT module name imported by the wrapper.
    pub import_module_name: String,
    /// WIT world name imported by the wrapper.
    pub import_world_name: String,
    /// Prefix for generated helper functions.
    pub helper_prefix: String,
}

/// Per-procedure values used by the Rust P2 wrapper template.
#[derive(Debug, Clone)]
pub struct ProcedureShim {
    /// Procedure name from the source code.
    pub name: String,
    /// Snake-cased WIT interface name.
    pub interface_name: String,
    /// Kebab-cased WIT interface name.
    pub wit_interface_name: String,
    /// Snake-cased adapter name.
    pub adapter_name: String,
    /// Kebab-cased adapter name.
    pub wit_adapter_name: String,
    /// Exported P2 function name.
    pub fn_exported_name: String,
    /// Inner procedure function name.
    pub fn_inner_name: String,
    /// Argument descriptor constant name.
    pub fn_argv_desc: String,
    /// Result descriptor constant name.
    pub fn_result_desc: String,
    /// Procedure descriptor constant name.
    pub fn_proc_desc: String,
    /// Exported function name in WIT kebab-case.
    pub wit_fn_exported_name: String,
    /// Export module name for the guest interface.
    pub export_mod_name: String,
    /// Guest struct name for the component world.
    pub guest_struct_name: String,
    /// Argument fields for the shim.
    pub argv_fields: Vec<ProcedureShimField>,
    /// Result fields for the shim.
    pub result_fields: Vec<ProcedureShimField>,
}

/// Input description for building a [`ProcedureShim`].
#[derive(Debug, Clone)]
pub struct ProcedureShimInput {
    /// Procedure name.
    pub name: String,
    /// Argument fields.
    pub argv_fields: Vec<ProcedureShimField>,
    /// Result fields.
    pub result_fields: Vec<ProcedureShimField>,
}

/// A single field in a procedure shim signature.
#[derive(Debug, Clone)]
pub struct ProcedureShimField {
    /// Field name.
    pub name: String,
    /// Rust expression that yields the field's [`mudu_type::data_type::DataType`].
    pub data_type_expr: String,
}

#[derive(Template)]
#[template(path = "procedure_shim/rust_p2_wrapper.rs.jinja", escape = "none")]
struct RustP2WrapperTemplate {
    procedures: Vec<ProcedureShim>,
    package_name: String,
    config: ProcedureShimConfig,
}

/// Render the Rust P2 wrapper source for the given procedures.
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
    .map_err(|e| {
        mudu_error!(
            ErrorCode::Encode,
            "render procedure shim rust wrapper error",
            e
        )
    })
}

/// Build the kebab-cased WIT interface name for a procedure.
pub fn procedure_interface_name(name: &str) -> String {
    format!("procedure-{}", to_kebab_case(name))
}

/// Build the kebab-cased WIT adapter name for a procedure.
pub fn procedure_adapter_name(name: &str) -> String {
    format!("adapter-{}", to_kebab_case(name))
}
