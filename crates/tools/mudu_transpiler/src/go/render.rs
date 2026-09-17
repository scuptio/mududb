//! Render the Go adapter source from parsed procedures.
//!
//! The generated `package main` file carries the wiring that used to be
//! hand-written in `main.go`: one `muduworld.Exports.Mp2<PascalProc>`
//! assignment per procedure (decode the wire parameters, call the same-named
//! user function, encode the result), the shared `invoke` driver, and an
//! empty `main`.
//!
//! Decode shapes per parameter type (all compiled against the fixed
//! `mudusys.go` surface — the `asXxx` cast helpers and the `domainError`
//! codes — plus the in-repo Go binding and, for user-defined types, the
//! project's mgen-generated types package):
//!
//! - scalars: `local, err := asXxx(p.Params[i])`.
//! - WIT records: `types.RecordFieldValues(p.Params[i])` unwraps the
//!   `uni-data-value` record-case envelope into the positional field map the
//!   generated `XxxFromValue` codec decodes.
//! - WIT enums: the generated `XxxFromValue` decodes the ordinal integer.
//! - `*T` options: a nil check wraps the inner decode and takes the address
//!   of the decoded value.
//!
//! A record-typed result encodes symmetrically: `XxxToValue` renders the
//! positional field map and `types.RecordFromFieldValues` wraps it back into
//! the record-case envelope; the `invoke` driver passes the resulting
//! `types.UniDataValue` through to the wire.

use crate::go::procedure::{GoProcedure, GoValueType};
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_kebab_case, to_pascal_case, to_snake_case};

/// Askama template for the Go adapter module.
#[derive(Template)]
#[template(path = "go/adapter.go.jinja", escape = "none")]
struct AdapterTemplate<'a> {
    binding_import: String,
    types_import: Option<String>,
    types_alias: String,
    uses_record_bridge: bool,
    world_kebab: String,
    procedures: &'a [AdapterProcedure],
}

struct AdapterProcedure {
    export_field: String,
    func_name: String,
    name: String,
    param_names: String,
    arg_count: usize,
    arg_locals: Vec<String>,
    decode_blocks: String,
    result_encode: String,
}

/// Render the Go adapter source that wires the discovered Mudu procedures
/// into the wit-bindgen-go `Exports` table.
///
/// `type_import` is the Go import path of the project's mgen-generated types
/// package (`--type-import`); it is required exactly when a procedure
/// signature references a user-defined type from `--type-wit`.
pub(super) fn render_adapter_source(
    procedures: &[GoProcedure],
    module_name: &str,
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
            "Go procedures reference user-defined types from --type-wit: pass --type-import with the Go import path of the mgen-generated types package (e.g. \"my_module/gentypes\")"
        ));
    }
    let uses_record_bridge = procedures.iter().any(|procedure| {
        procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_record_bridge())
            || procedure.return_value_type.uses_record_bridge()
    });
    let types_alias = type_import.map(go_import_alias).unwrap_or_default();
    let world_kebab = to_kebab_case(module_name);
    let adapter_procedures = procedures
        .iter()
        .map(|procedure| AdapterProcedure::from_procedure(procedure, &types_alias))
        .collect::<RS<Vec<_>>>()?;
    let mut source = AdapterTemplate {
        binding_import: format!("{module_name}/binding/mududb/{world_kebab}/{world_kebab}"),
        types_import: type_import.map(|path| path.to_string()),
        types_alias,
        uses_record_bridge,
        world_kebab,
        procedures: &adapter_procedures,
    }
    .render()
    .map_err(|e| mudu_error!(ErrorCode::Encode, "render go adapter error", e))?;
    // gofmt wants exactly one trailing newline; askama strips the template's.
    if !source.ends_with('\n') {
        source.push('\n');
    }
    Ok(source)
}

/// Derive the import alias for the generated types package from its import
/// path: the last path segment, sanitized to a Go identifier. Aliases that
/// would collide with the other imports of the generated adapter (`types`,
/// `muduworld`, `cm`) or the `main` package fall back to `mudutypes`. The
/// alias is reserved inside the generated closures (a parameter colliding
/// with it is renamed to the `arg<index>` fallback).
fn go_import_alias(import_path: &str) -> String {
    let segment = import_path
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("mudutypes");
    let alias: String = segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let reserved = ["types", "muduworld", "cm", "main"];
    if alias.is_empty()
        || alias
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(true)
        || reserved.contains(&alias.as_str())
    {
        return "mudutypes".to_string();
    }
    alias
}

/// Base indentation of the procedure closure body in the generated file.
const BODY_INDENT: &str = "\t\t\t";

impl AdapterProcedure {
    fn from_procedure(procedure: &GoProcedure, types_alias: &str) -> RS<Self> {
        let mut used = RESERVED_LOCALS
            .iter()
            .map(|name| name.to_string())
            .collect::<std::collections::HashSet<_>>();
        if !types_alias.is_empty() {
            used.insert(types_alias.to_string());
        }
        let mut arg_locals = Vec::with_capacity(procedure.params.len().saturating_sub(1));
        let mut decode_lines: Vec<String> = Vec::new();
        for (index, param) in procedure.params.iter().skip(1).enumerate() {
            let local = claim_local(&mut used, &param.name, index);
            // The record/option decode derives auxiliary names from the
            // local; reserve them so later parameters cannot collide.
            used.insert(format!("{local}Fields"));
            used.insert(format!("{local}Value"));
            decode_param(
                &param.value_type,
                &local,
                index,
                types_alias,
                BODY_INDENT,
                &mut decode_lines,
            )?;
            arg_locals.push(local);
        }
        let result_encode = encode_result(&procedure.return_value_type, types_alias, BODY_INDENT)?;
        Ok(Self {
            export_field: format!("Mp2{}", to_pascal_case(&procedure.name)),
            func_name: procedure.func_name.clone(),
            name: procedure.name.clone(),
            param_names: procedure
                .params
                .iter()
                .skip(1)
                .map(|param| to_snake_case(&param.name))
                .collect::<Vec<_>>()
                .join(", "),
            arg_count: procedure.params.len().saturating_sub(1),
            arg_locals,
            decode_blocks: decode_lines.join("\n"),
            result_encode,
        })
    }
}

fn err_check(lines: &mut Vec<String>, ind: &str) {
    lines.push(format!("{ind}if err != nil {{"));
    lines.push(format!("{ind}\treturn nil, err"));
    lines.push(format!("{ind}}}"));
}

fn invalid_shape(what: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::Internal, format!("go adapter render: {what}"))
}

/// Render the decode statements for one wire parameter.
fn decode_param(
    value_type: &GoValueType,
    local: &str,
    index: usize,
    alias: &str,
    ind: &str,
    lines: &mut Vec<String>,
) -> RS<()> {
    match value_type {
        GoValueType::Record(custom) => {
            lines.push(format!(
                "{ind}{local}Fields, err := types.RecordFieldValues(p.Params[{index}])"
            ));
            err_check(lines, ind);
            lines.push(format!(
                "{ind}{local}, err := {alias}.{}FromValue({local}Fields)",
                custom.name
            ));
            err_check(lines, ind);
        }
        GoValueType::Enum(custom) => {
            lines.push(format!(
                "{ind}{local}, err := {alias}.{}FromValue(p.Params[{index}])",
                custom.name
            ));
            err_check(lines, ind);
        }
        GoValueType::Option(inner) => {
            lines.push(format!(
                "{ind}var {local} {}",
                option_go_type(inner, alias)?
            ));
            lines.push(format!("{ind}if p.Params[{index}] != nil {{"));
            decode_option_inner(inner, local, index, alias, &format!("{ind}\t"), lines)?;
            lines.push(format!("{ind}}}"));
        }
        scalar => {
            let cast = scalar
                .cast_helper()
                .ok_or_else(|| invalid_shape("scalar parameter without a cast helper"))?;
            lines.push(format!("{ind}{local}, err := {cast}(p.Params[{index}])"));
            err_check(lines, ind);
        }
    }
    Ok(())
}

/// The body of an option decode: decode the present value into
/// `<local>Value` and take its address.
fn decode_option_inner(
    inner: &GoValueType,
    local: &str,
    index: usize,
    alias: &str,
    ind: &str,
    lines: &mut Vec<String>,
) -> RS<()> {
    match inner {
        GoValueType::Record(custom) => {
            lines.push(format!(
                "{ind}{local}Fields, err := types.RecordFieldValues(p.Params[{index}])"
            ));
            err_check(lines, ind);
            lines.push(format!(
                "{ind}{local}Value, err := {alias}.{}FromValue({local}Fields)",
                custom.name
            ));
            err_check(lines, ind);
        }
        GoValueType::Enum(custom) => {
            lines.push(format!(
                "{ind}{local}Value, err := {alias}.{}FromValue(p.Params[{index}])",
                custom.name
            ));
            err_check(lines, ind);
        }
        GoValueType::Boolean | GoValueType::Int64 | GoValueType::Float64 | GoValueType::Text => {
            let cast = inner
                .cast_helper()
                .ok_or_else(|| invalid_shape("option scalar without a cast helper"))?;
            lines.push(format!(
                "{ind}{local}Value, err := {cast}(p.Params[{index}])"
            ));
            err_check(lines, ind);
        }
        GoValueType::Binary | GoValueType::ObjectId | GoValueType::Option(_) => {
            return Err(invalid_shape(
                "option inner type the parser should have rejected",
            ));
        }
    }
    lines.push(format!("{ind}{local} = &{local}Value"));
    Ok(())
}

/// The Go type of an option parameter: a pointer to the scalar keyword or to
/// the generated custom type.
fn option_go_type(inner: &GoValueType, alias: &str) -> RS<String> {
    let ty = match inner {
        GoValueType::Boolean => "*bool".to_string(),
        GoValueType::Int64 => "*int64".to_string(),
        GoValueType::Float64 => "*float64".to_string(),
        GoValueType::Text => "*string".to_string(),
        GoValueType::Record(custom) | GoValueType::Enum(custom) => {
            format!("*{alias}.{}", custom.name)
        }
        GoValueType::Binary | GoValueType::ObjectId | GoValueType::Option(_) => {
            return Err(invalid_shape(
                "option inner type the parser should have rejected",
            ));
        }
    };
    Ok(ty)
}

/// Render the result encode: the statements after the procedure call
/// succeeded (`result` holds the typed return value).
fn encode_result(value_type: &GoValueType, alias: &str, ind: &str) -> RS<String> {
    match value_type {
        GoValueType::Record(custom) => {
            let mut lines = Vec::new();
            lines.push(format!(
                "{ind}resultWire, err := {alias}.{}ToValue(result)",
                custom.name
            ));
            err_check(&mut lines, ind);
            lines.push(format!(
                "{ind}return types.RecordFromFieldValues(resultWire)"
            ));
            Ok(lines.join("\n"))
        }
        GoValueType::Boolean
        | GoValueType::Int64
        | GoValueType::Float64
        | GoValueType::Text
        | GoValueType::Binary => Ok(format!("{ind}return result, nil")),
        // Enum, option and OID results are rejected by the parser (v1).
        GoValueType::Enum(_) | GoValueType::Option(_) | GoValueType::ObjectId => {
            Err(invalid_shape("result type the parser should have rejected"))
        }
    }
}

/// Names already taken inside the generated closure.
const RESERVED_LOCALS: &[&str] = &["p", "param", "err", "result", "resultWire"];

/// The generated local variable for one parameter: the declared name, unless
/// it collides with the fixed locals of the generated closure or an earlier
/// parameter — then an `arg<index>` fallback (suffixed until unique).
fn claim_local(used: &mut std::collections::HashSet<String>, name: &str, index: usize) -> String {
    let mut local = if used.contains(name) {
        format!("arg{index}")
    } else {
        name.to_string()
    };
    while used.contains(&local) {
        local.push('_');
    }
    used.insert(local.clone());
    local
}
