//! Render the AssemblyScript adapter source and the procedure world WIT from
//! parsed procedures.
//!
//! Decode shapes per parameter type (all compiled against the fixed binding
//! surface — the `Value`/`ValueList` DX types, the mp2 helpers of
//! `procedure.ts`, and, for user-defined types, the record bridge of
//! `record.ts` plus the project's mgen-generated types module):
//!
//! - scalars: `values.value(i).asXxx()` when the whole procedure rides the
//!   `ValueList` conversion, or `MuduValue.fromUniDataValue(param.param_list[i]).asXxx()`
//!   when a sibling parameter needs the raw envelope (the `ValueList`
//!   conversion rejects record datums).
//! - WIT records: `recordFieldValues(param.param_list[i])` transcodes the
//!   `uni-data-value` record-case envelope into the integer-keyed
//!   MessagePack map the generated `XxxCodec.decode` reads.
//! - WIT enums: the ordinal integer decodes via `as i32 as Xxx`.
//! - `T | null` options: an `isNull()`/`isNullDatum` guard picks `null` or
//!   the inner decode.
//!
//! A record-typed result encodes symmetrically: the generated `XxxCodec.encode`
//! renders the integer-keyed map and `recordFromFieldValues` wraps it back
//! into the record-case envelope, passed to `encodeProcedureOkUni`.

use crate::assemblyscript::procedure::{AsParam, AsProcedure, AsValueType};
use crate::common::ident::sanitize_identifier;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashSet;
use std::path::{Component, Path};

/// Askama template for the AssemblyScript adapter module.
#[derive(Template)]
#[template(path = "assemblyscript/adapter.ts.jinja", escape = "none")]
struct AdapterTemplate<'a> {
    binding_symbols: String,
    source_import_path: String,
    type_import_path: Option<String>,
    type_symbols: Option<String>,
    procedures: &'a [AdapterProcedure],
}

/// Render the AssemblyScript adapter source that imports and wraps the
/// discovered Mudu procedures.
///
/// `type_import` is the import specifier of the project's mgen-generated
/// AssemblyScript types module (`--type-import`, e.g. `"./gentypes"`); it is
/// required exactly when a procedure signature references a user-defined
/// type from `--type-wit`.
pub(super) fn render_adapter_source(
    input_path: &Path,
    output_path: &Path,
    procedures: &[AsProcedure],
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
            "AssemblyScript procedures reference user-defined types from --type-wit: pass --type-import with the import specifier of the mgen-generated types module (e.g. \"./gentypes\")"
        ));
    }
    let mut names = NameRegistry::default();
    let adapter_procedures = procedures
        .iter()
        .map(|procedure| AdapterProcedure::from_procedure(procedure, &mut names))
        .collect::<RS<Vec<_>>>()?;
    let binding_symbols = binding_symbols(procedures).join(",\n  ");
    let type_symbols = uses_custom_types.then(|| type_symbols(procedures).join(", "));
    AdapterTemplate {
        binding_symbols,
        source_import_path: source_import_path(input_path, output_path),
        type_import_path: type_import.map(|path| path.to_string()),
        type_symbols,
        procedures: &adapter_procedures,
    }
    .render()
    .map_err(|e| mudu_error!(ErrorCode::Encode, "render assemblyscript adapter error", e))
}

struct AdapterProcedure {
    name: String,
    source_alias: String,
    result_expr: String,
    return_value_ctor: String,
    returns_result: bool,
    returns_record: bool,
    record_codec: String,
    has_values_list: bool,
    args: Vec<AdapterArg>,
    call_args: Vec<String>,
}

struct AdapterArg {
    name: String,
    /// Explicit type annotation for option parameters (`": T | null"`):
    /// AssemblyScript cannot infer the null union of the decode ternary.
    ty_annotation: String,
    decode_expr: String,
}

impl AdapterProcedure {
    fn from_procedure(procedure: &AsProcedure, names: &mut NameRegistry) -> RS<Self> {
        // The raw-envelope path is needed when any parameter is a record (or
        // an option of one): the `ValueList` conversion rejects record
        // datums, so every parameter of such a procedure decodes straight
        // from `param.param_list`.
        let has_values_list = !procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_record_bridge());
        let value_args = procedure
            .params
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, param)| AdapterArg::from_param(index, param, has_values_list))
            .collect::<RS<Vec<_>>>()?;
        let mut call_args = Vec::with_capacity(procedure.params.len());
        call_args.push("id".to_string());
        call_args.extend(value_args.iter().map(|arg| arg.name.clone()));
        let result_expr = if procedure.returns_result {
            "result.unwrap()".to_string()
        } else {
            "result".to_string()
        };
        let (returns_record, record_codec, return_value_ctor) =
            match &procedure.return_value_type {
                AsValueType::Record(custom) => {
                    (true, format!("{}Codec", custom.name), String::new())
                }
                scalar => (
                    false,
                    String::new(),
                    scalar
                        .value_ctor()
                        .ok_or_else(|| {
                            mudu_error!(
                                ErrorCode::Internal,
                                "assemblyscript adapter render: result type the parser should have rejected"
                            )
                        })?
                        .to_string(),
                ),
            };

        Ok(Self {
            name: procedure.name.clone(),
            source_alias: names.claim("__mudu_proc_", &procedure.name),
            result_expr,
            return_value_ctor,
            returns_result: procedure.returns_result,
            returns_record,
            record_codec,
            has_values_list,
            args: value_args,
            call_args,
        })
    }
}

impl AdapterArg {
    fn from_param(value_index: usize, param: &AsParam, has_values_list: bool) -> RS<Self> {
        let decode_expr = decode_expr(&param.value_type, value_index, has_values_list)?;
        let ty_annotation = match &param.value_type {
            AsValueType::Option(inner) => format!(": {} | null", as_type_name(inner)?),
            _ => String::new(),
        };
        Ok(Self {
            name: param.name.clone(),
            ty_annotation,
            decode_expr,
        })
    }
}

/// The AssemblyScript type name of an option parameter's inner type (the
/// parser admits only the nullable reference types: string and records).
fn as_type_name(value_type: &AsValueType) -> RS<String> {
    match value_type {
        AsValueType::Text => Ok("string".to_string()),
        AsValueType::Record(custom) => Ok(custom.name.clone()),
        _ => Err(invalid_shape(
            "option inner type the parser should have rejected",
        )),
    }
}

fn invalid_shape(what: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::Internal,
        format!("assemblyscript adapter render: {what}")
    )
}

/// The full AssemblyScript expression decoding one wire parameter.
fn decode_expr(value_type: &AsValueType, index: usize, has_values_list: bool) -> RS<String> {
    // The base expression producing the parameter's `Value` view.
    let base = if has_values_list {
        format!("values.value({index})")
    } else {
        format!("MuduValue.fromUniDataValue(param.param_list[{index}])")
    };
    match value_type {
        AsValueType::Record(custom) => Ok(format!(
            "{}Codec.decode(new MpackReader(recordFieldValues(param.param_list[{index}])))",
            custom.name
        )),
        AsValueType::Enum(custom) => Ok(format!("{base}.asInt64() as i32 as {}", custom.name)),
        AsValueType::Option(inner) => match inner.as_ref() {
            AsValueType::Record(custom) => Ok(format!(
                "isNullDatum(param.param_list[{index}]) ? null : {}Codec.decode(new MpackReader(recordFieldValues(param.param_list[{index}])))",
                custom.name
            )),
            AsValueType::Text => Ok(format!("{base}.isNull() ? null : {base}.asText()")),
            _ => Err(invalid_shape(
                "option inner type the parser should have rejected",
            )),
        },
        scalar => {
            let getter = scalar
                .value_getter()
                .ok_or_else(|| invalid_shape("scalar parameter without a value getter"))?;
            Ok(format!("{base}.{getter}()"))
        }
    }
}

/// The symbols the adapter imports from `@mududb/mududb`, in the fixed
/// canonical order (the leading six are the historical set; the record
/// bridge symbols follow only when used).
fn binding_symbols(procedures: &[AsProcedure]) -> Vec<&'static str> {
    let any_record_param = procedures.iter().any(|procedure| {
        procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_record_bridge())
    });
    let any_record_return = procedures
        .iter()
        .any(|procedure| procedure.return_value_type.uses_record_bridge());
    let any_option_record_param = procedures.iter().any(|procedure| {
        procedure.params.iter().any(|param| {
            matches!(&param.value_type, AsValueType::Option(inner) if inner.uses_record_bridge())
        })
    });
    let any_values_list = procedures.iter().any(|procedure| {
        !procedure
            .params
            .iter()
            .any(|param| param.value_type.uses_record_bridge())
            || !procedure.return_value_type.uses_record_bridge()
    });
    let mut symbols = vec![
        "Oid",
        "Value as MuduValue",
        "decodeProcedureParam as __muduDecodeProcedureParam",
        "encodeProcedureErr as __muduEncodeProcedureErr",
    ];
    if any_values_list {
        symbols.insert(2, "ValueList");
        symbols.push("encodeProcedureOk as __muduEncodeProcedureOk");
    }
    if any_record_return {
        symbols.push("encodeProcedureOkUni as __muduEncodeProcedureOkUni");
    }
    if any_option_record_param {
        symbols.push("isNullDatum");
    }
    if any_record_param {
        symbols.push("recordFieldValues");
    }
    if any_record_return {
        symbols.push("recordFromFieldValues");
    }
    if any_record_param {
        symbols.push("MpackReader");
    }
    if any_record_return {
        symbols.push("MpackWriter");
        symbols.push("UniDataValue");
    }
    symbols
}

/// The symbols imported from the project's generated types module, in
/// first-use order: `<Name>Codec` for record types, `<Name>` for enums.
fn type_symbols(procedures: &[AsProcedure]) -> Vec<String> {
    let mut symbols: Vec<String> = Vec::new();
    let mut push = |symbol: String| {
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    };
    for procedure in procedures {
        for param in procedure.params.iter().skip(1) {
            collect_type_symbol(&param.value_type, &mut push);
        }
        collect_type_symbol(&procedure.return_value_type, &mut push);
    }
    symbols
}

fn collect_type_symbol(value_type: &AsValueType, push: &mut dyn FnMut(String)) {
    match value_type {
        AsValueType::Record(custom) => push(format!("{}Codec", custom.name)),
        AsValueType::Enum(custom) => push(custom.name.clone()),
        AsValueType::Option(inner) => {
            // An option-of-record parameter annotates the decode with the
            // record class itself (`: Profile | null`), not just its codec.
            if let AsValueType::Record(custom) = inner.as_ref() {
                push(custom.name.clone());
            }
            collect_type_symbol(inner, push);
        }
        _ => {}
    }
}

/// Render the byte-pipe procedure world WIT for the module: the guest imports
/// the host syscall interface `mududb:api/system` directly and exports one
/// root-level `mp2-<kebab>` byte-pipe function per procedure (the same world
/// shape the C# guest uses). Thin wrapper over the shared
/// [`crate::common::wit::render_wit`].
pub(super) fn render_wit(procedures: &[AsProcedure], module_name: &str) -> String {
    let names = procedures
        .iter()
        .map(|procedure| procedure.name.clone())
        .collect::<Vec<_>>();
    crate::common::wit::render_wit(&names, module_name, &[])
}

#[derive(Default)]
struct NameRegistry {
    names: HashSet<String>,
}

impl NameRegistry {
    fn claim(&mut self, prefix: &str, input: &str) -> String {
        let mut base = String::with_capacity(prefix.len() + input.len());
        base.push_str(prefix);
        base.push_str(&sanitize_identifier(input));

        let mut candidate = base.clone();
        let mut suffix = 2;
        while !self.names.insert(candidate.clone()) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        candidate
    }
}

fn source_import_path(input_path: &Path, output_path: &Path) -> String {
    let output_dir = output_path.parent().unwrap_or_else(|| Path::new("."));
    let mut path = relative_path(output_dir, input_path)
        .unwrap_or_else(|| input_path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    if let Some(stripped) = path.strip_suffix(".ts") {
        path = stripped.to_string();
    }
    if !path.starts_with('.') && !path.starts_with('/') {
        path = format!("./{path}");
    }
    path
}

fn relative_path(from_dir: &Path, to_file: &Path) -> Option<std::path::PathBuf> {
    let from_components = normalized_components(from_dir)?;
    let to_components = normalized_components(to_file)?;
    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(left, right)| left == right)
        .count();

    let mut out = std::path::PathBuf::new();
    for _ in common_len..from_components.len() {
        out.push("..");
    }
    for component in &to_components[common_len..] {
        out.push(component);
    }
    Some(out)
}

fn normalized_components(path: &Path) -> Option<Vec<String>> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => return None,
            Component::RootDir => components.push("/".to_string()),
            Component::CurDir => {}
            Component::ParentDir => components.push("..".to_string()),
            Component::Normal(value) => components.push(value.to_string_lossy().to_string()),
        }
    }
    Some(components)
}
