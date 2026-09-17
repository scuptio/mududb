//! Render the Python adapter source and the procedure world WIT from parsed
//! procedures.
//!
//! Decode shapes per parameter type (all interpreted against the fixed
//! adapter prelude — the `_from_uni` / `_to_uni` helpers — plus the in-repo
//! Python binding and, for user-defined types, the project's mgen-generated
//! types module):
//!
//! - scalars / unrecognized hints: `_from_uni(proc_param.param_list[i])`.
//! - WIT records: `mududb.codec.bridge.record_field_values` unwraps the
//!   `uni-data-value` record-case envelope into the positional field dict
//!   the generated `xxx_from_value` codec decodes.
//! - WIT enums: the generated `xxx_from_value` decodes the ordinal integer.
//! - options (`Optional[T]` / `T | None`): a `None` guard wraps the inner
//!   decode for record/enum inners; scalar inners decode through `_from_uni`
//!   like any plain argument (the Null scalar already unwraps to `None`).
//!
//! A record-typed result encodes symmetrically: `xxx_to_value` renders the
//! positional field dict and `record_from_field_values` wraps it back into
//! the record-case envelope.

use crate::common::ident::{is_valid_identifier, sanitize_identifier};
use crate::python::procedure::{PyProcedure, PyValueType};
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::path::Path;

/// Askama template for the Python adapter module.
#[derive(Template)]
#[template(path = "python/adapter.py.jinja", escape = "none")]
struct AdapterTemplate<'a> {
    source_module: String,
    extra_imports: String,
    procedures: &'a [AdapterProcedure],
}

/// Render the Python adapter source that imports and wraps the discovered
/// Mudu procedures.
///
/// The adapter imports the user module by the input file's stem (both the
/// adapter and the user module must be importable from the
/// `componentize-py -p` paths at component build time).
///
/// `type_import` is the dotted module path of the project's mgen-generated
/// types module (`--type-import`, e.g. `gentypes` or `gentypes.types`); it
/// is required exactly when a procedure signature references a user-defined
/// type from `--type-wit`.
pub(super) fn render_adapter_source(
    input_path: &Path,
    procedures: &[PyProcedure],
    type_import: Option<&str>,
) -> RS<String> {
    let uses_custom_types = procedures.iter().any(|procedure| {
        procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_custom_types())
            || procedure
                .return_value_types
                .iter()
                .any(|value_type| value_type.uses_custom_types())
    });
    if uses_custom_types && type_import.is_none() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "Python procedures reference user-defined types from --type-wit: pass --type-import with the dotted module path of the mgen-generated types module (e.g. \"gentypes\")"
        ));
    }
    let uses_record_bridge = procedures.iter().any(|procedure| {
        procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_record_bridge())
            || procedure
                .return_value_types
                .iter()
                .any(|value_type| value_type.uses_record_bridge())
    });
    let types_alias = match type_import {
        Some(type_import) => py_types_alias(type_import)?,
        None => String::new(),
    };
    let adapter_procedures = procedures
        .iter()
        .map(|procedure| AdapterProcedure::from_procedure(procedure, &types_alias))
        .collect::<Vec<_>>();
    AdapterTemplate {
        source_module: source_module(input_path)?,
        extra_imports: extra_imports(type_import, &types_alias, uses_record_bridge),
        procedures: &adapter_procedures,
    }
    .render()
    .map_err(|e| mudu_error!(ErrorCode::Encode, "render python adapter error", e))
}

/// The import section lines below the fixed `mududb` imports: the project's
/// mgen-generated types module (when `--type-import` is given) and the
/// binding's record bridge helpers (when a record type is decoded or
/// encoded). Empty when neither applies, keeping the historical output
/// byte-identical. A non-empty value ends with a newline so the template
/// keeps one blank line before the transport wiring comment.
fn extra_imports(type_import: Option<&str>, types_alias: &str, uses_record_bridge: bool) -> String {
    let mut imports = String::new();
    if let Some(type_import) = type_import {
        if type_import == types_alias {
            imports.push_str(&format!("import {type_import}\n"));
        } else {
            imports.push_str(&format!("import {type_import} as {types_alias}\n"));
        }
    }
    if uses_record_bridge {
        imports.push_str(
            "from mududb.codec.bridge import (\n    record_field_values as _record_fields,\n    record_from_field_values as _record_from_fields,\n)\n",
        );
    }
    imports
}

/// Derive the module alias for the generated types module from its dotted
/// path: a single-segment module (`gentypes`) imports under its own name; a
/// dotted path (`gentypes.types`) imports under the underscore-joined alias
/// (`gentypes_types`).
fn py_types_alias(type_import: &str) -> RS<String> {
    let segments: Vec<&str> = type_import.split('.').collect();
    if segments.is_empty() || !segments.iter().all(|segment| is_valid_identifier(segment)) {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("--type-import '{type_import}' is not a valid dotted Python module path")
        ));
    }
    Ok(segments.join("_"))
}

/// Base indentation of the procedure method body in the generated module.
const BODY_INDENT: &str = "            ";

struct AdapterProcedure {
    name: String,
    has_session: bool,
    /// Decode every parameter with an explicit statement (a user-defined
    /// type is present); `None` keeps the splat-comprehension call form.
    decode_blocks: Option<String>,
    call_args: String,
    /// The result value expressions inside the `encode_procedure_ok([...])`
    /// list; `None` keeps the generic `_result_values(result)` form.
    result_values: Option<String>,
}

impl AdapterProcedure {
    fn from_procedure(procedure: &PyProcedure, types_alias: &str) -> Self {
        let skip = if procedure.session_arg.is_some() {
            1
        } else {
            0
        };
        let explicit = procedure
            .params
            .iter()
            .skip(skip)
            .any(|param| needs_custom_decode(&param.value_type));
        let mut decode_blocks = None;
        let mut call_args = String::new();
        if explicit {
            let mut used = RESERVED_LOCALS
                .iter()
                .map(|name| name.to_string())
                .collect::<std::collections::HashSet<_>>();
            if !types_alias.is_empty() {
                used.insert(types_alias.to_string());
            }
            let mut locals = Vec::new();
            let mut lines = Vec::new();
            for (index, param) in procedure.params.iter().skip(skip).enumerate() {
                let local = claim_local(&mut used, &param.name, index);
                lines.push(format!(
                    "{BODY_INDENT}{}",
                    decode_line(&param.value_type, &local, index, types_alias)
                ));
                locals.push(local);
            }
            if procedure.session_arg.is_some() {
                call_args.push_str("proc_param.session, ");
            }
            call_args.push_str(&locals.join(", "));
            decode_blocks = Some(lines.join("\n"));
        }
        let custom_result = procedure
            .return_value_types
            .iter()
            .any(|value_type| value_type.uses_record_bridge());
        let result_values = if custom_result {
            let single = procedure.return_value_types.len() == 1;
            Some(
                procedure
                    .return_value_types
                    .iter()
                    .enumerate()
                    .map(|(index, value_type)| {
                        let expr = if single {
                            "result".to_string()
                        } else {
                            format!("result[{index}]")
                        };
                        result_value_expr(value_type, &expr, types_alias)
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        } else {
            None
        };
        Self {
            name: procedure.name.clone(),
            has_session: procedure.session_arg.is_some(),
            decode_blocks,
            call_args,
            result_values,
        }
    }
}

/// Whether the parameter needs its own decode statement (user-defined types
/// and option forms of them); plain scalars and scalar options unwrap with
/// the generic `_from_uni`.
fn needs_custom_decode(value_type: &PyValueType) -> bool {
    match value_type {
        PyValueType::Record(_) | PyValueType::Enum(_) => true,
        PyValueType::Option(inner) => needs_custom_decode(inner),
        _ => false,
    }
}

/// The decode statement (without indentation) for one wire parameter at
/// `proc_param.param_list[index]`.
fn decode_line(value_type: &PyValueType, local: &str, index: usize, alias: &str) -> String {
    let param_expr = format!("proc_param.param_list[{index}]");
    match value_type {
        PyValueType::Record(custom) => format!(
            "{local} = {alias}.{}_from_value(_record_fields({param_expr}))",
            custom.name_snake
        ),
        PyValueType::Enum(custom) => format!(
            "{local} = {alias}.{}_from_value(_from_uni({param_expr}))",
            custom.name_snake
        ),
        PyValueType::Option(inner) => match inner.as_ref() {
            PyValueType::Record(custom) => format!(
                "{local} = None if _from_uni({param_expr}) is None else {alias}.{}_from_value(_record_fields({param_expr}))",
                custom.name_snake
            ),
            PyValueType::Enum(custom) => format!(
                "{local} = None if _from_uni({param_expr}) is None else {alias}.{}_from_value(_from_uni({param_expr}))",
                custom.name_snake
            ),
            _ => format!("{local} = _from_uni({param_expr})"),
        },
        _ => format!("{local} = _from_uni({param_expr})"),
    }
}

/// The result value expression for one return value: a record wraps back
/// into the record-case envelope through the generated codec and the record
/// bridge; everything else goes through the generic `_to_uni`.
fn result_value_expr(value_type: &PyValueType, expr: &str, alias: &str) -> String {
    match value_type {
        PyValueType::Record(custom) => {
            format!(
                "_record_from_fields({alias}.{}_to_value({expr}))",
                custom.name_snake
            )
        }
        _ => format!("_to_uni({expr})"),
    }
}

/// Names already taken inside the generated method.
const RESERVED_LOCALS: &[&str] = &["param", "proc_param", "result", "e", "procedures"];

/// The generated local variable for one parameter: the declared name, unless
/// it collides with the fixed locals of the generated method or an earlier
/// parameter — then an `_arg<index>` fallback (suffixed until unique).
fn claim_local(used: &mut std::collections::HashSet<String>, name: &str, index: usize) -> String {
    let mut local = if used.contains(name) {
        format!("_arg{index}")
    } else {
        name.to_string()
    };
    while used.contains(&local) {
        local.push('_');
    }
    used.insert(local.clone());
    local
}

/// Render the byte-pipe procedure world WIT for the module: the guest imports
/// the host syscall interface `mududb:api/system` directly and exports one
/// root-level `mp2-<kebab>` byte-pipe function per procedure (the same world
/// shape the AssemblyScript and C# guests use). Thin wrapper over the shared
/// [`crate::common::wit::render_wit`].
pub(super) fn render_wit(procedures: &[PyProcedure], module_name: &str) -> String {
    let names = procedures
        .iter()
        .map(|procedure| procedure.name.clone())
        .collect::<Vec<_>>();
    crate::common::wit::render_wit(&names, module_name, &[])
}

fn source_module(input_path: &Path) -> RS<String> {
    let stem = input_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "Python input path has no valid file stem"))?;
    let module = sanitize_identifier(stem);
    if module != stem {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Python input file stem is not a valid module name",
            stem.to_string()
        ));
    }
    Ok(module)
}
