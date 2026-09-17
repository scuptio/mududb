//! Go wire-value conversion statement generation.
//!
//! The generated Go codecs follow the same value model as the Python
//! binding: every typed value converts through an exported `XxxToValue` /
//! `XxxFromValue` function pair whose wire form rides on native Go values
//! (`nil`/`bool`/integer widths/`float64`/`string`/`[]byte`/`[]any`/
//! `map[uint64]any`), and the generic `MpackWriter.WriteValue` /
//! `MpackReader.ReadValue` of the hand-written `codec` package serialize the
//! value model canonically. Because Go has no exceptions, this module emits
//! STATEMENTS (with explicit `err` propagation) rather than expressions; the
//! templates splice the pre-rendered blocks in.
//!
//! `func_bin` selects the func-level blob rule: `list<u8>`/`blob` values
//! encode as a MessagePack **bin** (`[]byte`) at the func level but as an
//! **array of ints** in the record/variant context (matching the Rust host's
//! `Vec<u8>` serde output). Decode is lenient and accepts both forms in
//! either context.
//!
//! Tuple surfaces are `[]any` carrying the typed element values (Go has no
//! tuple type), so tuple element conversions go through the checked
//! any-to-wire path ([`go_wire_from_any`]) on encode and [`go_from_value`]
//! on decode.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use std::collections::HashMap;

/// Func-signature context: `list<u8>`/`blob` encode as a MessagePack bin.
pub const GO_FUNC_BIN: bool = true;

/// Record/variant definition context: `list<u8>`/`blob` encode as an array.
pub const GO_RECORD_BIN: bool = false;

/// Import path of the hand-written MessagePack/MSSP runtime package the
/// generated code references (mirrors the Python binding's
/// `mududb.codec.mpack` import).
pub const GO_CODEC_IMPORT: &str = "github.com/ybbh/mududb_p/bindings/go/codec";

/// The Go package clause for a message-generation namespace.
///
/// The namespace is the explicit `--namespace` flag (the interface name is
/// deliberately NOT a default for Go: one output directory is one Go
/// package, and per-file interface names could split it). The canonical
/// binding interface (`universal`) and an empty namespace map to `types` —
/// the checked-in binding package clause, which keeps the binding
/// regeneration byte-stable. Any other namespace becomes the snake_case
/// package name of the project's generated types package (Go package names
/// are lower snake_case identifiers).
pub fn go_package_name(namespace: &str) -> String {
    if namespace.is_empty() || namespace == crate::src_gen::canonical::UNIVERSAL_INTERFACE {
        return "types".to_string();
    }
    let segments: Vec<String> = namespace
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|segment| !segment.is_empty())
        .map(to_snake_case)
        .collect();
    let joined = segments.join("_");
    if joined.is_empty() {
        "types".to_string()
    } else {
        joined
    }
}

/// The Go type used for values of the given WIT type in the generated
/// surface (record fields, variant payloads, func parameters/results).
///
/// Unlike the guest entity layer (`lang_def`: every integer rides on the
/// `mudusys.go` `int64` datum), the binding package uses the natural Go
/// widths so `u64` values keep their top bit.
pub fn go_type_name(ty: &UniDataType) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => "bool",
            UniScalar::U8 => "uint8",
            UniScalar::U16 => "uint16",
            UniScalar::U32 => "uint32",
            UniScalar::U64 => "uint64",
            // 128-bit integers travel as their 16-byte big-endian form.
            UniScalar::U128 | UniScalar::I128 => "[16]byte",
            UniScalar::I8 => "int8",
            UniScalar::I16 => "int16",
            UniScalar::I32 => "int32",
            UniScalar::I64 => "int64",
            UniScalar::F32 => "float32",
            UniScalar::F64 => "float64",
            UniScalar::Char
            | UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => "string",
            UniScalar::Blob => "[]byte",
        },
        UniDataType::Binary => "[]byte",
        UniDataType::Identifier(name) => return Ok(to_pascal_case(name)),
        UniDataType::Array(inner) => return Ok(format!("[]{}", go_type_name(inner)?)),
        UniDataType::Option(inner) => return Ok(format!("*{}", go_type_name(inner)?)),
        // Go has no tuple type: tuple surfaces are []any carrying the typed
        // element values.
        UniDataType::Tuple(_) => "[]any",
        UniDataType::Box(inner) => return go_type_name(inner),
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Go type name for {ty:?} is not implemented")
            ));
        }
    };
    Ok(s.to_string())
}

/// Convert raw WIT `//` comment text into Go `//` comment lines (the WIT
/// comment syntax is already Go's; a `///` doc prefix collapses to `//`).
pub fn go_comments(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("//") {
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                if rest.is_empty() {
                    "//".to_string()
                } else {
                    format!("//{rest}")
                }
            } else if line.is_empty() {
                "//".to_string()
            } else {
                format!("// {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Indent every line of a comment block by one tab (empty input stays
/// empty); used for struct-member doc comments.
pub fn indent_comments(comments: &str) -> String {
    if comments.is_empty() {
        return String::new();
    }
    comments
        .lines()
        .map(|line| format!("\t{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The exported Go identifier for a WIT name (PascalCase; exported idents
/// never collide with Go keywords, which are all lowercase).
pub fn go_exported(name: &str) -> String {
    to_pascal_case(name)
}

/// The unexported Go identifier for a WIT name (camelCase with a `_` suffix
/// when the name is a Go keyword, e.g. the `select` syscall parameter).
pub fn go_local_ident(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "type",
        "var",
    ];
    let pascal = to_pascal_case(name);
    let mut camel = String::with_capacity(pascal.len());
    for (i, c) in pascal.chars().enumerate() {
        if i == 0 {
            camel.extend(c.to_lowercase());
        } else {
            camel.push(c);
        }
    }
    if KEYWORDS.contains(&camel.as_str()) {
        format!("{camel}_")
    } else {
        camel
    }
}

/// The plain Go expression producing the proto3-style zero value of `ty`.
///
/// Identifier defaults are resolved through the type-kind registry: enums
/// construct their zero discriminant (`X(0)`), variants use the generated
/// `defaultX()` first-case factory, records use `X{}`. Unknown
/// (unregistered) identifiers fall back to `X{}` (correct for records, the
/// common cross-file reference).
pub fn go_zero(ty: &UniDataType, type_kinds: &HashMap<String, String>) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => "false".to_string(),
            UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64 => "0".to_string(),
            UniScalar::U128 | UniScalar::I128 => "[16]byte{}".to_string(),
            UniScalar::F32 | UniScalar::F64 => "0".to_string(),
            UniScalar::Char
            | UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => "\"\"".to_string(),
            UniScalar::Blob => "nil".to_string(),
        },
        UniDataType::Binary => "nil".to_string(),
        UniDataType::Array(_) => "nil".to_string(),
        UniDataType::Option(_) => "nil".to_string(),
        UniDataType::Tuple(elems) => {
            let mut parts = Vec::with_capacity(elems.len());
            for elem in elems {
                parts.push(go_zero(elem, type_kinds)?);
            }
            format!("[]any{{{}}}", parts.join(", "))
        }
        UniDataType::Box(inner) => go_zero(inner, type_kinds)?,
        UniDataType::Identifier(name) => {
            let pascal = to_pascal_case(name);
            match type_kinds.get(&pascal).map(|k| k.as_str()) {
                Some("enum") => format!("{pascal}(0)"),
                Some("variant") => format!("default{pascal}()"),
                _ => format!("{pascal}{{}}"),
            }
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Go default value for {ty:?} is not implemented")
            ));
        }
    };
    Ok(s)
}

/// Return the default-assignment expression (`defaultXxx()`) when `ty`
/// resolves through boxes to a registered variant identifier; `None`
/// otherwise. Go's zero value of a variant interface is `nil`, but the
/// wire-level default of an absent variant field is the first case with its
/// zero payload, so record decode seeds such fields explicitly.
pub fn go_variant_default(
    ty: &UniDataType,
    type_kinds: &HashMap<String, String>,
) -> RS<Option<String>> {
    match ty {
        UniDataType::Identifier(name) => {
            let pascal = to_pascal_case(name);
            if type_kinds.get(&pascal).map(|k| k.as_str()) == Some("variant") {
                Ok(Some(format!("default{pascal}()")))
            } else {
                Ok(None)
            }
        }
        UniDataType::Box(inner) => go_variant_default(inner, type_kinds),
        _ => Ok(None),
    }
}

/// Emit the statements converting the typed value at `src` into its wire
/// value assigned to `dst` (a Go assignable place: a variable, a map index
/// or a slice index). The enclosing function is a `XxxToValue` /
/// `EncodeXxx*` function returning `(any, error)` / `([]byte, error)`, so
/// every failure path is `return nil, err`.
///
/// `tmp` is a per-function counter making every emitted temporary unique;
/// `indent` is the base indentation (tabs) of the emitted block.
pub fn go_to_value(
    ty: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    indent: &str,
    func_bin: bool,
) -> RS<String> {
    let mut lines = Vec::new();
    to_value_at(ty, src, dst, tmp, indent, func_bin, &mut lines)?;
    Ok(lines.join("\n"))
}

fn to_value_at(
    ty: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    ind: &str,
    func_bin: bool,
    lines: &mut Vec<String>,
) -> RS<()> {
    match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool
            | UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64
            | UniScalar::F64
            | UniScalar::Char
            | UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => {
                lines.push(format!("{ind}{dst} = {src}"));
            }
            UniScalar::F32 => {
                // the F32 marker makes the generic writer emit f32 (0xCA)
                lines.push(format!("{ind}{dst} = codec.F32({src})"));
            }
            UniScalar::Blob => {
                to_value_blob(src, dst, ind, func_bin, lines, "");
            }
            UniScalar::U128 | UniScalar::I128 => {
                // the 16-byte big-endian form; a bin at func level, an int
                // array in the record context
                to_value_blob(src, dst, ind, func_bin, lines, "[:]");
            }
        },
        UniDataType::Binary => {
            to_value_blob(src, dst, ind, func_bin, lines, "");
        }
        UniDataType::Identifier(name) => {
            let n = next_tmp(tmp);
            lines.push(format!(
                "{ind}_c{n}, err := {}ToValue({src})",
                to_pascal_case(name)
            ));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\treturn nil, err"));
            lines.push(format!("{ind}}}"));
            lines.push(format!("{ind}{dst} = _c{n}"));
        }
        UniDataType::Array(inner) => {
            let n = next_tmp(tmp);
            lines.push(format!("{ind}_v{n} := make([]any, len({src}))"));
            lines.push(format!("{ind}for _i{n}, _e{n} := range {src} {{"));
            to_value_at(
                inner,
                &format!("_e{n}"),
                &format!("_v{n}[_i{n}]"),
                tmp,
                &format!("{ind}\t"),
                func_bin,
                lines,
            )?;
            lines.push(format!("{ind}}}"));
            lines.push(format!("{ind}{dst} = _v{n}"));
        }
        UniDataType::Option(inner) => {
            // Absent options assign nothing: record map keys stay omitted
            // (proto3 presence semantics) and slice/tuple slots keep their
            // nil zero value.
            lines.push(format!("{ind}if {src} != nil {{"));
            to_value_at(
                inner,
                &format!("*{src}"),
                dst,
                tmp,
                &format!("{ind}\t"),
                func_bin,
                lines,
            )?;
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Tuple(elems) => {
            let n = next_tmp(tmp);
            lines.push(format!("{ind}_v{n} := make([]any, {})", elems.len()));
            for (i, elem) in elems.iter().enumerate() {
                // tuple surfaces are []any: elements arrive as `any` holding
                // the typed value, so they convert through the checked
                // any-to-wire path
                wire_from_any_at(
                    elem,
                    &format!("{src}[{i}]"),
                    &format!("_v{n}[{i}]"),
                    tmp,
                    ind,
                    func_bin,
                    lines,
                )?;
            }
            lines.push(format!("{ind}{dst} = _v{n}"));
        }
        UniDataType::Box(inner) => {
            to_value_at(inner, src, dst, tmp, ind, func_bin, lines)?;
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Go wire conversion for {ty:?} is not implemented")
            ));
        }
    }
    Ok(())
}

fn to_value_blob(
    src: &str,
    dst: &str,
    ind: &str,
    func_bin: bool,
    lines: &mut Vec<String>,
    suffix: &str,
) {
    if func_bin {
        lines.push(format!("{ind}{dst} = {src}{suffix}"));
    } else {
        // record-context list<u8>: a MessagePack ARRAY of u8, not a bin blob
        lines.push(format!("{ind}{dst} = blobToArrayValue({src}{suffix})"));
    }
}

/// Emit the statements converting the wire value at `src` (`any`) into the
/// typed value assigned to `dst`. `err_ret` is the enclosing function's
/// error-return statement text using the `err` variable (e.g.
/// `return x, err` in a record decoder, `return WireResult{}, err` in a
/// result decoder); `src` is assumed to come from the lenient reader.
pub fn go_from_value(
    ty: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    indent: &str,
    err_ret: &str,
) -> RS<String> {
    let mut lines = Vec::new();
    from_value_at(ty, src, dst, tmp, indent, err_ret, &mut lines)?;
    Ok(lines.join("\n"))
}

fn from_value_at(
    ty: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    ind: &str,
    err_ret: &str,
    lines: &mut Vec<String>,
) -> RS<()> {
    match ty {
        UniDataType::Scalar(scalar) => {
            let helper = match scalar {
                UniScalar::Bool => "boolChecked",
                UniScalar::U8 => "u8Checked",
                UniScalar::U16 => "u16Checked",
                UniScalar::U32 => "u32Checked",
                UniScalar::U64 => "u64Checked",
                UniScalar::U128 => "u128FromValue",
                UniScalar::I8 => "i8Checked",
                UniScalar::I16 => "i16Checked",
                UniScalar::I32 => "i32Checked",
                UniScalar::I64 => "i64Checked",
                UniScalar::I128 => "i128FromValue",
                UniScalar::F32 => "f32FromValue",
                UniScalar::F64 => "f64Checked",
                UniScalar::Char => "charChecked",
                UniScalar::String
                | UniScalar::Numeric
                | UniScalar::Date
                | UniScalar::Time
                | UniScalar::Timestamp
                | UniScalar::TimestampTz => "strChecked",
                // Both the bin and the array-of-u8 form are accepted on
                // decode in either context (lenient).
                UniScalar::Blob => "blobFromValue",
            };
            lines.push(format!("{ind}{dst}, err = {helper}({src})"));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\t{err_ret}"));
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Binary => {
            lines.push(format!("{ind}{dst}, err = blobFromValue({src})"));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\t{err_ret}"));
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Identifier(name) => {
            lines.push(format!(
                "{ind}{dst}, err = {}FromValue({src})",
                to_pascal_case(name)
            ));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\t{err_ret}"));
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Array(inner) => {
            let n = next_tmp(tmp);
            lines.push(format!("{ind}_l{n}, err := expectList({src})"));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\t{err_ret}"));
            lines.push(format!("{ind}}}"));
            lines.push(format!(
                "{ind}_v{n} := make([]{}, len(_l{n}))",
                go_type_name(inner)?
            ));
            lines.push(format!("{ind}for _i{n}, _e{n} := range _l{n} {{"));
            from_value_at(
                inner,
                &format!("_e{n}"),
                &format!("_v{n}[_i{n}]"),
                tmp,
                &format!("{ind}\t"),
                err_ret,
                lines,
            )?;
            lines.push(format!("{ind}}}"));
            lines.push(format!("{ind}{dst} = _v{n}"));
        }
        UniDataType::Option(inner) => {
            let n = next_tmp(tmp);
            lines.push(format!("{ind}if {src} != nil {{"));
            lines.push(format!("{ind}\tvar _t{n} {}", go_type_name(inner)?));
            from_value_at(
                inner,
                src,
                &format!("_t{n}"),
                tmp,
                &format!("{ind}\t"),
                err_ret,
                lines,
            )?;
            lines.push(format!("{ind}\t{dst} = &_t{n}"));
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Tuple(elems) => {
            let n = next_tmp(tmp);
            lines.push(format!(
                "{ind}_s{n}, err := expectSeq({src}, {})",
                elems.len()
            ));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\t{err_ret}"));
            lines.push(format!("{ind}}}"));
            let mut parts = Vec::with_capacity(elems.len());
            for (i, elem) in elems.iter().enumerate() {
                let var = format!("_t{n}_{i}");
                lines.push(format!("{ind}var {var} {}", go_type_name(elem)?));
                from_value_at(elem, &format!("_s{n}[{i}]"), &var, tmp, ind, err_ret, lines)?;
                parts.push(var);
            }
            lines.push(format!("{ind}{dst} = []any{{{}}}", parts.join(", ")));
        }
        UniDataType::Box(inner) => {
            from_value_at(inner, src, dst, tmp, ind, err_ret, lines)?;
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Go wire conversion for {ty:?} is not implemented")
            ));
        }
    }
    Ok(())
}

/// Emit the statements converting an `any` place holding a TYPED value (a
/// `WireResult.Value` or a tuple element) into the wire value assigned to
/// `dst`. The value is type-asserted to the Go surface type and then flows
/// through [`go_to_value`]; assertion failures return a `typeError` via the
/// enclosing `([]byte, error)` function (`return nil, err`).
pub fn go_wire_from_any(
    ty: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    indent: &str,
    func_bin: bool,
) -> RS<String> {
    let mut lines = Vec::new();
    wire_from_any_at(ty, src, dst, tmp, indent, func_bin, &mut lines)?;
    Ok(lines.join("\n"))
}

fn wire_from_any_at(
    ty: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    ind: &str,
    func_bin: bool,
    lines: &mut Vec<String>,
) -> RS<()> {
    match ty {
        UniDataType::Scalar(scalar) => {
            let n = next_tmp(tmp);
            let helper = match scalar {
                UniScalar::Bool => "boolChecked",
                UniScalar::U8 => "u8Checked",
                UniScalar::U16 => "u16Checked",
                UniScalar::U32 => "u32Checked",
                UniScalar::U64 => "u64Checked",
                UniScalar::U128 => "u128FromValue",
                UniScalar::I8 => "i8Checked",
                UniScalar::I16 => "i16Checked",
                UniScalar::I32 => "i32Checked",
                UniScalar::I64 => "i64Checked",
                UniScalar::I128 => "i128FromValue",
                UniScalar::F32 => "f32FromValue",
                UniScalar::F64 => "f64Checked",
                UniScalar::Char => "charChecked",
                UniScalar::String
                | UniScalar::Numeric
                | UniScalar::Date
                | UniScalar::Time
                | UniScalar::Timestamp
                | UniScalar::TimestampTz => "strChecked",
                UniScalar::Blob => {
                    let h = if func_bin {
                        // func-context blob: the wire form is the []byte itself
                        "bytesChecked"
                    } else {
                        "blobFromValue"
                    };
                    lines.push(format!("{ind}_t{n}, err := {h}({src})"));
                    lines.push(format!("{ind}if err != nil {{"));
                    lines.push(format!("{ind}\treturn nil, err"));
                    lines.push(format!("{ind}}}"));
                    if func_bin {
                        lines.push(format!("{ind}{dst} = _t{n}"));
                    } else {
                        lines.push(format!("{ind}{dst} = blobToArrayValue(_t{n})"));
                    }
                    return Ok(());
                }
            };
            lines.push(format!("{ind}_t{n}, err := {helper}({src})"));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\treturn nil, err"));
            lines.push(format!("{ind}}}"));
            match scalar {
                UniScalar::F32 => {
                    lines.push(format!("{ind}{dst} = codec.F32(_t{n})"));
                }
                UniScalar::U128 | UniScalar::I128 => {
                    if func_bin {
                        lines.push(format!("{ind}{dst} = _t{n}[:]"));
                    } else {
                        lines.push(format!("{ind}{dst} = blobToArrayValue(_t{n}[:])"));
                    }
                }
                _ => {
                    lines.push(format!("{ind}{dst} = _t{n}"));
                }
            }
        }
        UniDataType::Binary => {
            let n = next_tmp(tmp);
            let h = if func_bin {
                "bytesChecked"
            } else {
                "blobFromValue"
            };
            lines.push(format!("{ind}_t{n}, err := {h}({src})"));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\treturn nil, err"));
            lines.push(format!("{ind}}}"));
            if func_bin {
                lines.push(format!("{ind}{dst} = _t{n}"));
            } else {
                lines.push(format!("{ind}{dst} = blobToArrayValue(_t{n})"));
            }
        }
        UniDataType::Identifier(_) | UniDataType::Array(_) => {
            let n = next_tmp(tmp);
            let go_ty = go_type_name(ty)?;
            lines.push(format!("{ind}_t{n}, _ok{n} := {src}.({go_ty})"));
            lines.push(format!("{ind}if !_ok{n} {{"));
            lines.push(format!("{ind}\terr := typeError(\"{go_ty}\", {src})"));
            lines.push(format!("{ind}\treturn nil, err"));
            lines.push(format!("{ind}}}"));
            to_value_at(ty, &format!("_t{n}"), dst, tmp, ind, func_bin, lines)?;
        }
        UniDataType::Option(inner) => {
            // WireResult.Value carries option payloads unwrapped: nil means
            // absent, anything else is the typed inner value.
            lines.push(format!("{ind}if {src} == nil {{"));
            lines.push(format!("{ind}\t{dst} = nil"));
            lines.push(format!("{ind}}} else {{"));
            wire_from_any_at(inner, src, dst, tmp, &format!("{ind}\t"), func_bin, lines)?;
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Tuple(elems) => {
            let n = next_tmp(tmp);
            lines.push(format!("{ind}_t{n}, _ok{n} := {src}.([]any)"));
            lines.push(format!(
                "{ind}if !_ok{n} || len(_t{n}) != {} {{",
                elems.len()
            ));
            lines.push(format!(
                "{ind}\terr := typeError(\"a {}-element array\", {src})",
                elems.len()
            ));
            lines.push(format!("{ind}\treturn nil, err"));
            lines.push(format!("{ind}}}"));
            let m = next_tmp(tmp);
            lines.push(format!("{ind}_v{m} := make([]any, {})", elems.len()));
            for (i, elem) in elems.iter().enumerate() {
                wire_from_any_at(
                    elem,
                    &format!("_t{n}[{i}]"),
                    &format!("_v{m}[{i}]"),
                    tmp,
                    ind,
                    func_bin,
                    lines,
                )?;
            }
            lines.push(format!("{ind}{dst} = _v{m}"));
        }
        UniDataType::Box(inner) => {
            wire_from_any_at(inner, src, dst, tmp, ind, func_bin, lines)?;
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Go wire conversion for {ty:?} is not implemented")
            ));
        }
    }
    Ok(())
}

fn next_tmp(tmp: &mut usize) -> usize {
    let n = *tmp;
    *tmp += 1;
    n
}

/// Pad identifier names with spaces so each contiguous run (runs are broken
/// by members carrying doc comments) aligns its follow-up column, matching
/// gofmt's tabwriter alignment of const/struct blocks; the generated files
/// are gofmt-clean without post-processing.
pub fn pad_ident_runs(has_comments: &[bool], names: &[String]) -> Vec<String> {
    let mut padded = Vec::with_capacity(names.len());
    let mut i = 0;
    while i < names.len() {
        if has_comments[i] {
            padded.push(names[i].clone());
            i += 1;
            continue;
        }
        let mut j = i;
        while j < names.len() && !has_comments[j] {
            j += 1;
        }
        let width = names[i..j].iter().map(|name| name.len()).max().unwrap_or(0);
        for name in &names[i..j] {
            padded.push(format!("{name:width$}", width = width));
        }
        i = j;
    }
    padded
}

#[cfg(test)]
mod tests {
    use super::{go_local_ident, go_package_name, go_to_value, go_type_name, go_zero};
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn package_names_follow_the_namespace_flag() {
        // the canonical binding surface and the empty namespace keep the
        // checked-in `types` package clause
        assert_eq!(go_package_name(""), "types");
        assert_eq!(go_package_name("universal"), "types");
        // project namespaces become lower snake_case Go package names
        assert_eq!(go_package_name("gentypes"), "gentypes");
        assert_eq!(go_package_name("wallet-types"), "wallet_types");
        assert_eq!(go_package_name("WalletTypes"), "wallet_types");
        assert_eq!(go_package_name("my.game.types"), "my_game_types");
        // degenerate input falls back to the canonical clause
        assert_eq!(go_package_name("--"), "types");
    }

    #[test]
    fn type_names_use_native_widths() -> RS<()> {
        assert_eq!(go_type_name(&UniDataType::Scalar(UniScalar::Bool))?, "bool");
        assert_eq!(
            go_type_name(&UniDataType::Scalar(UniScalar::U64))?,
            "uint64"
        );
        assert_eq!(go_type_name(&UniDataType::Scalar(UniScalar::I64))?, "int64");
        assert_eq!(
            go_type_name(&UniDataType::Scalar(UniScalar::F32))?,
            "float32"
        );
        assert_eq!(
            go_type_name(&UniDataType::Scalar(UniScalar::Blob))?,
            "[]byte"
        );
        assert_eq!(
            go_type_name(&UniDataType::Identifier("uni-oid".to_string()))?,
            "UniOid"
        );
        assert_eq!(
            go_type_name(&UniDataType::Array(Box::new(UniDataType::Scalar(
                UniScalar::U64
            ))))?,
            "[]uint64"
        );
        assert_eq!(
            go_type_name(&UniDataType::Option(Box::new(UniDataType::Binary)))?,
            "*[]byte"
        );
        Ok(())
    }

    #[test]
    fn blob_context_switches_bin_and_array() -> RS<()> {
        let blob = UniDataType::Binary;
        let mut tmp = 0;
        assert_eq!(
            go_to_value(&blob, "key", "_body[2]", &mut tmp, "\t", true)?,
            "\t_body[2] = key"
        );
        let mut tmp = 0;
        assert_eq!(
            go_to_value(&blob, "x.Key", "_d[2]", &mut tmp, "\t", false)?,
            "\t_d[2] = blobToArrayValue(x.Key)"
        );
        Ok(())
    }

    #[test]
    fn composite_to_value_nests() -> RS<()> {
        let ty = UniDataType::Array(Box::new(UniDataType::Scalar(UniScalar::U64)));
        let mut tmp = 0;
        assert_eq!(
            go_to_value(&ty, "x.Select", "_body[4]", &mut tmp, "\t", true)?,
            "\t_v0 := make([]any, len(x.Select))\n\
             \tfor _i0, _e0 := range x.Select {\n\
             \t\t_v0[_i0] = _e0\n\
             \t}\n\
             \t_body[4] = _v0"
        );
        Ok(())
    }

    #[test]
    fn option_to_value_assigns_only_when_present() -> RS<()> {
        let ty = UniDataType::Option(Box::new(UniDataType::Binary));
        let mut tmp = 0;
        assert_eq!(
            go_to_value(&ty, "x.ParamDesc", "_d[4]", &mut tmp, "\t", false)?,
            "\tif x.ParamDesc != nil {\n\
             \t\t_d[4] = blobToArrayValue(*x.ParamDesc)\n\
             \t}"
        );
        Ok(())
    }

    #[test]
    fn zero_values_resolve_type_kinds() -> RS<()> {
        let mut kinds = std::collections::HashMap::new();
        kinds.insert("UniScalar".to_string(), "enum".to_string());
        kinds.insert("UniDataType".to_string(), "variant".to_string());
        kinds.insert("UniOid".to_string(), "record".to_string());
        assert_eq!(go_zero(&UniDataType::Scalar(UniScalar::U64), &kinds)?, "0");
        assert_eq!(
            go_zero(&UniDataType::Identifier("uni-scalar".to_string()), &kinds)?,
            "UniScalar(0)"
        );
        assert_eq!(
            go_zero(
                &UniDataType::Identifier("uni-data-type".to_string()),
                &kinds
            )?,
            "defaultUniDataType()"
        );
        assert_eq!(
            go_zero(&UniDataType::Identifier("uni-oid".to_string()), &kinds)?,
            "UniOid{}"
        );
        Ok(())
    }

    #[test]
    fn local_ident_avoids_keywords() {
        assert_eq!(go_local_ident("select"), "select_");
        assert_eq!(go_local_ident("worker-id"), "workerId");
        assert_eq!(go_local_ident("oid"), "oid");
    }

    #[test]
    fn ident_runs_pad_like_gofmt() {
        let names = vec![
            "ErrCode".to_string(),
            "ErrMsg".to_string(),
            "ErrDetails".to_string(),
            "X".to_string(),
        ];
        // a commented member forms its own run, so only the first two pad
        let padded = super::pad_ident_runs(&[false, false, true, false], &names);
        assert_eq!(padded[0], "ErrCode");
        assert_eq!(padded[1], "ErrMsg ");
        assert_eq!(padded[2], "ErrDetails");
        assert_eq!(padded[3], "X");
    }
}
