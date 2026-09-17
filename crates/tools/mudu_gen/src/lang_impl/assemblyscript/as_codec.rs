//! AssemblyScript MessagePack codec snippet generation.
//!
//! Produces explicit `MpackWriter`/`MpackReader` statement lists (the runtime
//! in `crates/sdk/bindings/assemblyscript/assembly/mpack.ts` exposes only
//! explicit per-type methods, so templates cannot use a generic
//! `write<T>`/`read<T>` style). Shared by the record/variant/enum/func
//! AssemblyScript templates.
//!
//! The pinned dual semantics of `list<u8>` are controlled by
//! [`AsCodecStyle::bytes_as_array`]: inside record/variant definitions a
//! `list<u8>` value is a plain MessagePack ARRAY of `u8` (matching the Rust
//! host's `Vec<u8>` serde output), while at func signature level it is a
//! MessagePack **bin** blob.

use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use std::collections::HashMap;

/// Codec generation style selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsCodecStyle {
    /// Whether `list<u8>`/`blob` values encode as plain MessagePack arrays of
    /// `u8` (record/variant context) instead of MessagePack bin (func
    /// context).
    pub bytes_as_array: bool,
}

/// Record/variant definition context: `list<u8>` is a MessagePack array.
pub const AS_STYLE_RECORD: AsCodecStyle = AsCodecStyle {
    bytes_as_array: true,
};

/// Func signature context: `list<u8>` is a MessagePack bin blob.
pub const AS_STYLE_FUNC: AsCodecStyle = AsCodecStyle {
    bytes_as_array: false,
};

/// Names tuple holder classes (`Array<any>` is not viable for typed,
/// byte-exact codecs, so each tuple type gets a small generated class with
/// `f0..fN` fields).
pub trait AsTupleNamer {
    /// Return the holder class name for a tuple with the given element types.
    fn tuple_name(&mut self, elems: &[UniDataType]) -> RS<String>;
}

/// A generated tuple holder class (`f0..fN` fields).
#[derive(Debug, Clone)]
pub struct AsTupleHolder {
    /// Holder class name.
    pub name: String,
    /// Field types, parallel to the tuple elements; fields are named `f0..fN`.
    pub field_types: Vec<String>,
    /// Field default-value expressions, parallel to `field_types`.
    pub field_defaults: Vec<String>,
}

/// An [`AsTupleNamer`] that names holders within one top-level signature
/// position (`begin` resets the position base) and accumulates the holder
/// declarations, deduplicated by name, for the template to emit. Names are
/// resolved by (position base, tuple shape) so repeated passes (type
/// computation, encode, decode) over the same position resolve identical
/// names.
pub struct CollectingTupleNamer {
    base: String,
    /// (base, shape debug key, name) triples in registration order.
    positions: Vec<(String, String, String)>,
    holders: Vec<AsTupleHolder>,
    /// Type-kind registry (PascalCase name → `enum`/`variant`/`record`) used
    /// to render identifier default values (enums are not constructable).
    pub kinds: HashMap<String, String>,
}

impl Default for CollectingTupleNamer {
    fn default() -> Self {
        Self::new()
    }
}

impl CollectingTupleNamer {
    /// Create an empty namer.
    pub fn new() -> Self {
        Self::with_kinds(HashMap::new())
    }

    /// Create a namer with a type-kind registry for identifier defaults.
    pub fn with_kinds(kinds: HashMap<String, String>) -> Self {
        Self {
            base: String::new(),
            positions: Vec::new(),
            holders: Vec::new(),
            kinds,
        }
    }

    /// Start a new top-level position (e.g. one func parameter or one record
    /// field) whose tuple holders are named `#{base}Item`, `#{base}Item2`, …
    pub fn begin(&mut self, base: String) {
        self.base = base;
    }

    /// The accumulated holder declarations.
    pub fn holders(&self) -> &[AsTupleHolder] {
        &self.holders
    }
}

impl AsTupleNamer for CollectingTupleNamer {
    fn tuple_name(&mut self, elems: &[UniDataType]) -> RS<String> {
        let key = format!("{:?}", elems);
        if let Some((_, _, name)) = self
            .positions
            .iter()
            .find(|(base, shape, _)| *base == self.base && *shape == key)
        {
            return Ok(name.clone());
        }
        let index = self
            .positions
            .iter()
            .filter(|(base, _, _)| *base == self.base)
            .count()
            + 1;
        let name = if index == 1 {
            format!("{}Item", self.base)
        } else {
            format!("{}Item{}", self.base, index)
        };
        self.positions.push((self.base.clone(), key, name.clone()));
        let mut field_types = Vec::with_capacity(elems.len());
        let mut field_defaults = Vec::with_capacity(elems.len());
        for elem in elems {
            field_types.push(as_codec_type(elem, self)?);
            field_defaults.push(as_codec_default(elem, self)?);
        }
        self.holders.push(AsTupleHolder {
            name: name.clone(),
            field_types,
            field_defaults,
        });
        Ok(name)
    }
}

/// The AssemblyScript type name of a value in codec position.
pub fn as_codec_type(ty: &UniDataType, namer: &mut dyn AsTupleNamer) -> RS<String> {
    let s = match ty {
        UniDataType::Binary => "Uint8Array".to_string(),
        UniDataType::Array(inner) => format!("Array<{}>", as_codec_type(inner, namer)?),
        UniDataType::Option(inner) => format!("{} | null", as_codec_type(inner, namer)?),
        UniDataType::Tuple(elems) => namer.tuple_name(elems)?,
        UniDataType::Box(inner) => as_codec_type(inner, namer)?,
        UniDataType::Scalar(UniScalar::Blob) => "Uint8Array".to_string(),
        _ => uni_data_type_to_name(ty, &LangKind::AssemblyScript)?,
    };
    Ok(s)
}

/// The AssemblyScript default-value expression of a codec type.
///
/// Identifier defaults are resolved through the namer's type-kind registry:
/// enums are value types and cannot be constructed with `new X()`, so they
/// use a zero reinterpret cast; records/variants use `new X()`; unknown
/// (unregistered) identifiers fall back to `new X()` (correct for records,
/// which are the common cross-file reference).
pub fn as_codec_default(ty: &UniDataType, namer: &mut CollectingTupleNamer) -> RS<String> {
    let s = match ty {
        UniDataType::Binary | UniDataType::Scalar(UniScalar::Blob) => {
            "new Uint8Array(0)".to_string()
        }
        UniDataType::Array(inner) => {
            format!("new Array<{}>(0)", as_codec_type(inner, namer)?)
        }
        UniDataType::Option(_) => "null".to_string(),
        UniDataType::Tuple(elems) => format!("new {}()", namer.tuple_name(elems)?),
        UniDataType::Box(inner) => as_codec_default(inner, namer)?,
        UniDataType::Identifier(name) => {
            let pascal = to_pascal_case(name);
            match namer.kinds.get(&pascal).map(|k| k.as_str()) {
                Some("enum") => format!("changetype<{}>(0)", pascal),
                _ => format!("new {}()", pascal),
            }
        }
        _ => crate::lang_impl::lang::lang_data_type::assemblyscript_default_value_expr(ty)?,
    };
    Ok(s)
}

/// Append the statements encoding `expr` (a value of type `ty`) to `out`,
/// one line per statement, indented by `indent` spaces.
pub fn as_encode_stmts(
    style: AsCodecStyle,
    ty: &UniDataType,
    expr: &str,
    indent: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = " ".repeat(indent);
    match ty {
        UniDataType::Scalar(scalar) => {
            let stmt = match scalar {
                UniScalar::Bool => format!("writer.writeBool({});", expr),
                UniScalar::U8 | UniScalar::U16 | UniScalar::U32 => {
                    format!("writer.writeU64({} as u64);", expr)
                }
                UniScalar::U64 => format!("writer.writeU64({});", expr),
                UniScalar::I8 | UniScalar::I16 | UniScalar::I32 => {
                    format!("writer.writeI64({} as i64);", expr)
                }
                UniScalar::I64 => format!("writer.writeI64({});", expr),
                UniScalar::F32 => format!("writer.writeF32({});", expr),
                UniScalar::F64 => format!("writer.writeF64({});", expr),
                UniScalar::Char
                | UniScalar::String
                | UniScalar::Numeric
                | UniScalar::Date
                | UniScalar::Time
                | UniScalar::Timestamp
                | UniScalar::TimestampTz => {
                    format!("writer.writeString({});", expr)
                }
                UniScalar::Blob => {
                    return as_encode_bin(style, expr, indent, out);
                }
                UniScalar::U128 | UniScalar::I128 => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "AssemblyScript codec does not support 128-bit integers"
                    ));
                }
            };
            out.push(format!("{}{}", pad, stmt));
        }
        UniDataType::Binary => {
            return as_encode_bin(style, expr, indent, out);
        }
        UniDataType::Identifier(name) => {
            out.push(format!(
                "{}{}Codec.encode({}, writer);",
                pad,
                to_pascal_case(name),
                expr
            ));
        }
        UniDataType::Array(inner) => {
            out.push(format!("{}writer.writeArrayHeader({}.length);", pad, expr));
            out.push(format!(
                "{}for (let i = 0; i < {}.length; i++) {{",
                pad, expr
            ));
            out.push(format!("{}    const item = {}[i];", pad, expr));
            as_encode_stmts(style, inner, "item", indent + 4, out)?;
            out.push(format!("{}}}", pad));
        }
        UniDataType::Option(inner) => {
            out.push(format!("{}{{", pad));
            out.push(format!("{}    const opt = {};", pad, expr));
            out.push(format!("{}    if (opt === null) {{", pad));
            out.push(format!("{}        writer.writeNil();", pad));
            out.push(format!("{}    }} else {{", pad));
            as_encode_stmts(style, inner, "opt!", indent + 8, out)?;
            out.push(format!("{}    }}", pad));
            out.push(format!("{}}}", pad));
        }
        UniDataType::Tuple(elems) => {
            out.push(format!("{}writer.writeArrayHeader({});", pad, elems.len()));
            for (i, elem) in elems.iter().enumerate() {
                as_encode_stmts(style, elem, &format!("{}.f{}", expr, i), indent, out)?;
            }
        }
        UniDataType::Box(inner) => {
            return as_encode_stmts(style, inner, expr, indent, out);
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "AssemblyScript codec does not support {:?} in this position",
                    ty
                )
            ));
        }
    }
    Ok(())
}

fn as_encode_bin(style: AsCodecStyle, expr: &str, indent: usize, out: &mut Vec<String>) -> RS<()> {
    let pad = " ".repeat(indent);
    if style.bytes_as_array {
        out.push(format!("{}writer.writeArrayHeader({}.length);", pad, expr));
        out.push(format!(
            "{}for (let i = 0; i < {}.length; i++) {{",
            pad, expr
        ));
        out.push(format!("{}    writer.writeU64({}[i]);", pad, expr));
        out.push(format!("{}}}", pad));
    } else {
        out.push(format!("{}writer.writeBin({});", pad, expr));
    }
    Ok(())
}

/// Append the statements decoding a value of type `ty` into `target` (an
/// assignable l-value expression) to `out`, one line per statement, indented
/// by `indent` spaces.
pub fn as_decode_stmts(
    style: AsCodecStyle,
    ty: &UniDataType,
    target: &str,
    indent: usize,
    out: &mut Vec<String>,
    namer: &mut dyn AsTupleNamer,
) -> RS<()> {
    let pad = " ".repeat(indent);
    match ty {
        UniDataType::Scalar(scalar) => {
            let stmt = match scalar {
                UniScalar::Bool => format!("{} = reader.readBool();", target),
                UniScalar::U8 => format!("{} = reader.readU64() as u8;", target),
                UniScalar::U16 => format!("{} = reader.readU64() as u16;", target),
                UniScalar::U32 => format!("{} = reader.readU64() as u32;", target),
                UniScalar::U64 => format!("{} = reader.readU64();", target),
                UniScalar::I8 => format!("{} = reader.readI64() as i8;", target),
                UniScalar::I16 => format!("{} = reader.readI64() as i16;", target),
                UniScalar::I32 => format!("{} = reader.readI64() as i32;", target),
                UniScalar::I64 => format!("{} = reader.readI64();", target),
                UniScalar::F32 => format!("{} = reader.readF32();", target),
                UniScalar::F64 => format!("{} = reader.readF64();", target),
                UniScalar::Char
                | UniScalar::String
                | UniScalar::Numeric
                | UniScalar::Date
                | UniScalar::Time
                | UniScalar::Timestamp
                | UniScalar::TimestampTz => {
                    format!("{} = reader.readString();", target)
                }
                UniScalar::Blob => {
                    return as_decode_bin(style, target, indent, out);
                }
                UniScalar::U128 | UniScalar::I128 => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "AssemblyScript codec does not support 128-bit integers"
                    ));
                }
            };
            out.push(format!("{}{}", pad, stmt));
        }
        UniDataType::Binary => {
            return as_decode_bin(style, target, indent, out);
        }
        UniDataType::Identifier(name) => {
            out.push(format!(
                "{}{} = {}Codec.decode(reader);",
                pad,
                target,
                to_pascal_case(name)
            ));
        }
        UniDataType::Array(inner) => {
            out.push(format!("{}{{", pad));
            out.push(format!(
                "{}    const n = reader.readArrayHeader() as i32;",
                pad
            ));
            out.push(format!(
                "{}    const arr = new Array<{}>(n);",
                pad,
                as_codec_type(inner, namer)?
            ));
            out.push(format!("{}    for (let i = 0; i < n; i++) {{", pad));
            if let UniDataType::Tuple(elems) = inner.as_ref() {
                // Array elements of reference type start as `null`; decode
                // into a fresh holder before storing.
                out.push(format!(
                    "{}        const item = new {}();",
                    pad,
                    namer.tuple_name(elems)?
                ));
                as_decode_tuple_body(style, elems, "item", indent + 8, out, namer)?;
                out.push(format!("{}        arr[i] = item;", pad));
            } else {
                as_decode_stmts(style, inner, "arr[i]", indent + 8, out, namer)?;
            }
            out.push(format!("{}    }}", pad));
            out.push(format!("{}    {} = arr;", pad, target));
            out.push(format!("{}}}", pad));
        }
        UniDataType::Option(inner) => {
            out.push(format!("{}if (reader.tryNil()) {{", pad));
            out.push(format!("{}    {} = null;", pad, target));
            out.push(format!("{}}} else {{", pad));
            as_decode_stmts(style, inner, target, indent + 4, out, namer)?;
            out.push(format!("{}}}", pad));
        }
        UniDataType::Tuple(elems) => {
            as_decode_tuple_body(style, elems, target, indent, out, namer)?;
        }
        UniDataType::Box(inner) => {
            return as_decode_stmts(style, inner, target, indent, out, namer);
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "AssemblyScript codec does not support {:?} in this position",
                    ty
                )
            ));
        }
    }
    Ok(())
}

fn as_decode_tuple_body(
    style: AsCodecStyle,
    elems: &[UniDataType],
    target: &str,
    indent: usize,
    out: &mut Vec<String>,
    namer: &mut dyn AsTupleNamer,
) -> RS<()> {
    let pad = " ".repeat(indent);
    out.push(format!("{}{{", pad));
    out.push(format!("{}    const tc = reader.readArrayHeader();", pad));
    out.push(format!("{}    if (tc !== {}) {{", pad, elems.len()));
    out.push(format!(
        "{}        throw new Error(`Expected array of length {}, got ${{tc}}`);",
        pad,
        elems.len()
    ));
    out.push(format!("{}    }}", pad));
    for (i, elem) in elems.iter().enumerate() {
        as_decode_stmts(
            style,
            elem,
            &format!("{}.f{}", target, i),
            indent + 4,
            out,
            namer,
        )?;
    }
    out.push(format!("{}}}", pad));
    Ok(())
}

fn as_decode_bin(
    style: AsCodecStyle,
    target: &str,
    indent: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = " ".repeat(indent);
    if style.bytes_as_array {
        out.push(format!("{}{{", pad));
        out.push(format!(
            "{}    const n = reader.readArrayHeader() as i32;",
            pad
        ));
        out.push(format!("{}    const arr = new Uint8Array(n);", pad));
        out.push(format!("{}    for (let i = 0; i < n; i++) {{", pad));
        out.push(format!("{}        arr[i] = reader.readU64() as u8;", pad));
        out.push(format!("{}    }}", pad));
        out.push(format!("{}    {} = arr;", pad, target));
        out.push(format!("{}}}", pad));
    } else {
        out.push(format!("{}{} = reader.readBin();", pad, target));
    }
    Ok(())
}

/// Join snippet lines into a single template-ready string.
pub fn as_join(lines: &[String]) -> String {
    lines.join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{AS_STYLE_FUNC, AS_STYLE_RECORD, AsTupleNamer, as_decode_stmts, as_encode_stmts};
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    struct TestNamer;

    impl AsTupleNamer for TestNamer {
        fn tuple_name(&mut self, _elems: &[UniDataType]) -> RS<String> {
            Ok("TestTuple".to_string())
        }
    }

    fn encode(style: super::AsCodecStyle, ty: &UniDataType) -> Vec<String> {
        let mut out = Vec::new();
        as_encode_stmts(style, ty, "value.x", 4, &mut out).unwrap();
        out
    }

    #[test]
    fn record_context_binary_encodes_as_u8_array() {
        let lines = encode(AS_STYLE_RECORD, &UniDataType::Binary);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("writeArrayHeader(value.x.length)"))
        );
        assert!(lines.iter().any(|l| l.contains("writeU64(value.x[i])")));
        assert!(!lines.iter().any(|l| l.contains("writeBin")));
    }

    #[test]
    fn func_context_binary_encodes_as_bin() {
        let lines = encode(AS_STYLE_FUNC, &UniDataType::Binary);
        assert_eq!(lines, vec!["    writer.writeBin(value.x);".to_string()]);
    }

    #[test]
    fn scalars_use_explicit_methods() {
        let lines = encode(AS_STYLE_FUNC, &UniDataType::Scalar(UniScalar::U32));
        assert_eq!(
            lines,
            vec!["    writer.writeU64(value.x as u64);".to_string()]
        );
        let lines = encode(AS_STYLE_FUNC, &UniDataType::Scalar(UniScalar::I64));
        assert_eq!(lines, vec!["    writer.writeI64(value.x);".to_string()]);
    }

    #[test]
    fn decode_array_of_tuple_uses_holder_class() {
        let ty = UniDataType::Array(Box::new(UniDataType::Tuple(vec![
            UniDataType::Scalar(UniScalar::U64),
            UniDataType::Binary,
        ])));
        let mut out = Vec::new();
        as_decode_stmts(AS_STYLE_FUNC, &ty, "value.key", 4, &mut out, &mut TestNamer).unwrap();
        let joined = out.join("\n");
        assert!(joined.contains("new Array<TestTuple>(n)"));
        assert!(joined.contains("const item = new TestTuple();"));
        assert!(joined.contains("item.f0 = reader.readU64();"));
        assert!(joined.contains("item.f1 = reader.readBin();"));
    }
}
