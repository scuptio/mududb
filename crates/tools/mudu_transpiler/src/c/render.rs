//! Render the C adapter source from parsed procedures.
//!
//! The generated translation unit carries, per procedure: an `extern`
//! declaration of the user implementation, a static check preamble derived
//! from the marker annotation (arity plus `mudu_datum.kind` per parameter),
//! and the `mp2-<kebab>` export wrapper that drives `mudu_run_proc`.
//!
//! Record parameters pass through as the `MUDU_DATUM_RECORD` datum kind (the
//! procedure decodes the envelope itself, composing the binding's record
//! bridge with the mgen-generated `<type>_decode`); enum parameters ride as
//! plain `MUDU_DATUM_I64` ordinals; an `option<T>` parameter additionally
//! accepts `MUDU_DATUM_NULL`. When any procedure references a `--type-wit`
//! type, the adapter includes the mgen-generated types header named by
//! `--type-import`, which makes the wiring visible to (and validated by) the
//! wrapper translation unit's compile.

use crate::c::procedure::{CProcedure, CValueType};
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_kebab_case;
use mudu_contract::procedure::proc;

/// Askama template for the C adapter translation unit.
#[derive(Template)]
#[template(path = "c/adapter.c.jinja", escape = "none")]
struct AdapterTemplate<'a> {
    type_import: Option<&'a str>,
    procedures: &'a [AdapterProcedure],
}

struct AdapterProcedure {
    name: String,
    export_name: String,
    wrapper_name: String,
    check_expr: String,
    signature_text: String,
}

/// Render the C adapter source that wraps the discovered Mudu procedures.
///
/// `type_import` is the include specifier of the project's mgen-generated
/// types header (`--type-import`, e.g. `"gentypes/Types.h"`); it is required
/// exactly when a procedure signature references a user-defined type from
/// `--type-wit`.
pub(super) fn render_adapter_source(
    procedures: &[CProcedure],
    type_import: Option<&str>,
) -> RS<String> {
    let uses_custom_types = procedures.iter().any(|procedure| {
        procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_custom_types())
            || procedure.return_value_type.uses_custom_types()
    });
    if uses_custom_types && type_import.is_none() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "C procedures reference user-defined types from --type-wit: pass --type-import with the include specifier of the mgen-generated types header (e.g. \"gentypes/Types.h\")"
        ));
    }
    let adapter_procedures = procedures
        .iter()
        .map(AdapterProcedure::from_procedure)
        .collect::<Vec<_>>();
    AdapterTemplate {
        type_import,
        procedures: &adapter_procedures,
    }
    .render()
    .map_err(|e| mudu_error!(ErrorCode::Encode, "render c adapter error", e))
}

impl AdapterProcedure {
    fn from_procedure(procedure: &CProcedure) -> Self {
        let mut check_expr = format!("param->n_params != {}u", procedure.params.len());
        for (index, param) in procedure.params.iter().enumerate() {
            check_expr.push_str(" || ");
            check_expr.push_str(&datum_check(index, &param.value_type));
        }
        let signature_text = format!(
            "({})",
            procedure
                .params
                .iter()
                .map(|param| format!("{}: {}", param.name, param.value_type.canonical_name()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Self {
            name: procedure.name.clone(),
            export_name: to_kebab_case(&format!("{}{}", proc::MUDU_PROC_P2_PREFIX, procedure.name)),
            wrapper_name: format!("{}{}", proc::MUDU_PROC_P2_PREFIX, procedure.name),
            check_expr,
            signature_text,
        }
    }
}

/// The `mudu_datum.kind` guard for one parameter: record parameters pass
/// through as `MUDU_DATUM_RECORD`, enum parameters as `MUDU_DATUM_I64`, and
/// an option parameter additionally accepts `MUDU_DATUM_NULL`.
fn datum_check(index: usize, value_type: &CValueType) -> String {
    let kind_check = format!("param->params[{index}].kind != {}", value_type.datum_kind());
    if value_type.is_nullable() {
        format!("({kind_check} && param->params[{index}].kind != MUDU_DATUM_NULL)")
    } else {
        kind_check
    }
}
