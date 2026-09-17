//! Render the C# adapter source from parsed procedures.
//!
//! The generated file implements the componentize-dotnet exports class the
//! wit-bindgen C# backend calls by naming convention: namespace
//! `<PascalWorld>World`, class `<PascalWorld>WorldExportsImpl`, with one
//! `Mp2<PascalProc>` method per procedure (mirroring the hand-written
//! `WorldExportsImpl.cs` shape the C# projects used before mtp).
//!
//! Decode shapes per parameter type:
//!
//! - scalars: the `AsXxx` cast helpers over `p.Params[i]`.
//! - WIT records: a per-type `As<Name>` helper composing the record bridge
//!   (`MuduSys.RecordFieldValues`) with the generated `<Name>Formatter`.
//! - WIT enums: the ordinal integer casts to the enum type.
//!
//! Result shapes: `long` results keep the historical `Invoke` +
//! `MuduSys.EncodeProcedureOk(long)` path; every other result type goes
//! through `InvokeObj` with a per-type encode lambda (scalar overloads of
//! `MuduSys.EncodeProcedureOk`, or the record bridge
//! `MuduSys.EncodeProcedureOkRecord` over the generated formatter's output).

use crate::csharp::procedure::{CsProcedure, CsValueType};
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_kebab_case, to_pascal_case};

/// Askama template for the C# adapter module.
#[derive(Template)]
#[template(path = "csharp/adapter.cs.jinja", escape = "none")]
struct AdapterTemplate<'a> {
    source_namespace: String,
    world_namespace: String,
    world_kebab: String,
    exports_class: String,
    exception_type: String,
    uses_message_pack: bool,
    uses_invoke_obj: bool,
    procedures: &'a [AdapterProcedure],
    casts: &'a [CastHelper],
    record_helpers: &'a [RecordHelper],
}

struct AdapterProcedure {
    export_method: String,
    method_name: String,
    args: Vec<AdapterArg>,
    returns_long: bool,
    result_encode: String,
}

struct AdapterArg {
    expr: String,
}

struct RecordHelper {
    helper_name: String,
    type_name: String,
    formatter_name: String,
}

struct CastHelper {
    name: &'static str,
    cs_type: &'static str,
    needs_nullable_suffix: bool,
    message: &'static str,
}

/// Render the C# adapter source that wraps the discovered Mudu procedures
/// into the `<PascalWorld>WorldExportsImpl` exports class.
///
/// `type_import` is the C# namespace of the project's mgen-generated types
/// (`--type-import`, e.g. `"WalletCs.Types"`); it is required exactly when a
/// procedure signature references a user-defined type from `--type-wit`.
pub(super) fn render_adapter_source(
    procedures: &[CsProcedure],
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
            "C# procedures reference user-defined types from --type-wit: pass --type-import with the C# namespace of the mgen-generated types (e.g. \"WalletCs.Types\")"
        ));
    }
    let type_namespace = type_import.unwrap_or_default();
    let world_kebab = to_kebab_case(module_name);
    let pascal_world = to_pascal_case(&world_kebab);
    let adapter_procedures = procedures
        .iter()
        .map(|procedure| AdapterProcedure::from_procedure(procedure, type_namespace))
        .collect::<RS<Vec<_>>>()?;
    let casts = used_cast_helpers(procedures);
    let record_helpers = used_record_helpers(procedures, type_namespace);
    let uses_message_pack = procedures.iter().any(|procedure| {
        procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_record_bridge())
            || procedure.return_value_type.uses_record_bridge()
    });
    let uses_invoke_obj = procedures
        .iter()
        .any(|procedure| !matches!(procedure.return_value_type, CsValueType::Int64));
    AdapterTemplate {
        source_namespace: to_pascal_case(module_name),
        world_namespace: format!("{pascal_world}World"),
        world_kebab,
        exports_class: format!("{pascal_world}WorldExportsImpl"),
        exception_type: format!("{}Exception", to_pascal_case(module_name)),
        uses_message_pack,
        uses_invoke_obj,
        procedures: &adapter_procedures,
        casts: &casts,
        record_helpers: &record_helpers,
    }
    .render()
    .map_err(|e| mudu_error!(ErrorCode::Encode, "render csharp adapter error", e))
}

impl AdapterProcedure {
    fn from_procedure(procedure: &CsProcedure, type_namespace: &str) -> RS<Self> {
        let args = procedure
            .params
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, param)| {
                Ok(AdapterArg {
                    expr: decode_expr(&param.value_type, index, type_namespace)?,
                })
            })
            .collect::<RS<Vec<_>>>()?;
        let returns_long = matches!(procedure.return_value_type, CsValueType::Int64);
        let result_encode = if returns_long {
            String::new()
        } else {
            result_encode_expr(&procedure.return_value_type, type_namespace)?
        };
        Ok(Self {
            export_method: format!("Mp2{}", to_pascal_case(&procedure.name)),
            method_name: procedure.method_name.clone(),
            args,
            returns_long,
            result_encode,
        })
    }
}

fn invalid_shape(what: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::Internal,
        format!("csharp adapter render: {what}")
    )
}

/// The full C# expression decoding one wire parameter.
fn decode_expr(value_type: &CsValueType, index: usize, type_namespace: &str) -> RS<String> {
    match value_type {
        CsValueType::Record(custom) => Ok(format!("As{}(p.Params[{index}])", custom.name)),
        CsValueType::Enum(custom) => Ok(format!(
            "(global::{type_namespace}.{})AsI64(p.Params[{index}])",
            custom.name
        )),
        scalar => {
            let cast = scalar
                .cast_helper()
                .ok_or_else(|| invalid_shape("scalar parameter without a cast helper"))?;
            Ok(format!("{cast}(p.Params[{index}])"))
        }
    }
}

/// The encode lambda passed to `InvokeObj` for a non-`long` result.
fn result_encode_expr(value_type: &CsValueType, type_namespace: &str) -> RS<String> {
    match value_type {
        CsValueType::Boolean => Ok("value => MuduSys.EncodeProcedureOk((bool)value!)".to_string()),
        CsValueType::Float64 => {
            Ok("value => MuduSys.EncodeProcedureOk((double)value!)".to_string())
        }
        CsValueType::Text => Ok("value => MuduSys.EncodeProcedureOk((string)value!)".to_string()),
        CsValueType::Binary => Ok("value => MuduSys.EncodeProcedureOk((byte[])value!)".to_string()),
        CsValueType::Enum(custom) => Ok(format!(
            "value => MuduSys.EncodeProcedureOk((int)(global::{type_namespace}.{})value!)",
            custom.name
        )),
        CsValueType::Record(custom) => Ok(format!(
            "value =>\n        {{\n            var buffer = new global::System.Buffers.ArrayBufferWriter<byte>();\n            var writer = new MessagePackWriter(buffer);\n            new global::{type_namespace}.{name}Formatter().Serialize(ref writer, (global::{type_namespace}.{name})value!, null!);\n            writer.Flush();\n            return MuduSys.EncodeProcedureOkRecord(buffer.WrittenMemory.ToArray());\n        }}",
            name = custom.name
        )),
        // long results ride the legacy Invoke path; OID results are rejected
        // by the parser.
        CsValueType::Int64 | CsValueType::ObjectId => {
            Err(invalid_shape("result type the parser should have rejected"))
        }
    }
}

/// The per-record-type decode helpers the discovered procedures need, in
/// first-use order: `As<Name>` composing the record bridge with the
/// generated formatter.
fn used_record_helpers(procedures: &[CsProcedure], type_namespace: &str) -> Vec<RecordHelper> {
    let mut helpers: Vec<RecordHelper> = Vec::new();
    for procedure in procedures {
        for param in procedure.params.iter().skip(1) {
            if let CsValueType::Record(custom) = &param.value_type
                && !helpers
                    .iter()
                    .any(|helper| helper.helper_name == format!("As{}", custom.name))
            {
                helpers.push(RecordHelper {
                    helper_name: format!("As{}", custom.name),
                    type_name: format!("global::{type_namespace}.{}", custom.name),
                    formatter_name: format!("global::{type_namespace}.{}Formatter", custom.name),
                });
            }
        }
    }
    helpers
}

/// The cast helpers the discovered procedures need, in a fixed canonical
/// order so the generated source is deterministic.
fn used_cast_helpers(procedures: &[CsProcedure]) -> Vec<CastHelper> {
    const ALL: &[CastHelper] = &[
        CastHelper {
            name: "AsBool",
            cs_type: "bool",
            needs_nullable_suffix: true,
            message: "expected a bool parameter",
        },
        CastHelper {
            name: "AsI64",
            cs_type: "long",
            needs_nullable_suffix: true,
            message: "expected an i64 parameter",
        },
        CastHelper {
            name: "AsF64",
            cs_type: "double",
            needs_nullable_suffix: true,
            message: "expected an f64 parameter",
        },
        CastHelper {
            name: "AsString",
            cs_type: "string",
            needs_nullable_suffix: false,
            message: "expected a string parameter",
        },
        CastHelper {
            name: "AsBytes",
            cs_type: "byte[]",
            needs_nullable_suffix: false,
            message: "expected a binary parameter",
        },
        CastHelper {
            name: "AsOid",
            cs_type: "MuduOid",
            needs_nullable_suffix: true,
            message: "expected an oid parameter",
        },
    ];
    let used = |name: &str| {
        procedures
            .iter()
            .flat_map(|procedure| procedure.params.iter().skip(1))
            .any(|param| param.value_type.used_cast_helper() == Some(name))
    };
    ALL.iter()
        .filter(|cast| used(cast.name))
        .map(|cast| CastHelper {
            name: cast.name,
            cs_type: cast.cs_type,
            needs_nullable_suffix: cast.needs_nullable_suffix,
            message: cast.message,
        })
        .collect()
}
