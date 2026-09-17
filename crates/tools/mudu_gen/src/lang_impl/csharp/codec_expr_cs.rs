//! Resolver-free C# MessagePack emission expressions.
//!
//! The generated C# formatters historically called
//! `MessagePackSerializer.Serialize/Deserialize(..., options)` for every
//! nested field. With `null` options that touches
//! `MessagePackSerializer.DefaultOptions`, whose construction runs the
//! `MessagePackSecurity` static initializer (a SipHash keyed by
//! `RandomNumberGenerator`) and the Reflection.Emit resolver chain — both
//! unsupported on the wasi-wasm NativeAOT target
//! (`PlatformNotSupportedException`).
//!
//! This module emits static, per-type read/write statements instead: raw
//! `MessagePackWriter`/`MessagePackReader` calls for scalars and strings,
//! direct `XFormatter` instances for user-defined types, and explicit loops
//! for lists, options and tuples. The emitted calls match the canonical MSSP
//! wire form (minimal-width integers, single-character strings for `char`);
//! MessagePack-CSharp's typed integer reads already accept any marker width,
//! which is exactly the lenient decode the wire format requires. Booleans
//! are the one library read that is strict, so the emitted bool read accepts
//! the procedure byte-pipe i32 0/1 form as well (the host's uni-data-value
//! vocabulary has no Bool case).
//! Types without a resolver-free mapping (128-bit scalars, inline
//! records/results, nested containers inside list elements or tuple items)
//! fall back to `MessagePackSerializer` with the caller-supplied options;
//! none of those shapes occur in the `mudu_binding` WIT set.

use crate::lang_impl::lang::lang_data_type::{
    csharp_is_reference_type, csharp_is_reference_type_with_kinds, uni_data_type_to_name,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use std::collections::HashMap;

/// Emit C# statements (each line indented by `ind`, trailing newline) writing
/// `value` — a C# expression of WIT type `ty` — to `writer`.
pub fn serialize_stmts(
    ty: &UniDataType,
    value: &str,
    ind: &str,
    tmp: &mut usize,
    type_kinds: &HashMap<String, String>,
) -> RS<String> {
    let mut out = String::new();
    match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool
            | UniScalar::U8
            | UniScalar::I8
            | UniScalar::U16
            | UniScalar::I16
            | UniScalar::U32
            | UniScalar::I32
            | UniScalar::U64
            | UniScalar::I64
            | UniScalar::F32
            | UniScalar::F64 => {
                line(&mut out, ind, &format!("writer.Write({value});"));
            }
            UniScalar::Char => {
                // A WIT char travels as a single-character MessagePack
                // string (MessagePack-CSharp's own char write uses a ushort
                // marker, which is NOT the wire form).
                line(&mut out, ind, &format!("writer.Write({value}.ToString());"));
            }
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz
            | UniScalar::Blob => {
                // The resolver's reference-type formatters encode `null` as
                // nil; guard explicitly to keep that behavior.
                nil_guard(&mut out, ind, tmp, value, &mut |out, inner_ind, _| {
                    line(out, inner_ind, &format!("writer.Write({value});"));
                    Ok(())
                })?;
            }
            UniScalar::U128 | UniScalar::I128 => {
                serialize_fallback(&mut out, ind, value);
            }
        },
        UniDataType::Binary => {
            // Record-context `binary` is `List<byte>`: a MessagePack ARRAY of
            // u8, matching the resolver's `ListFormatter<byte>`.
            nil_guard(&mut out, ind, tmp, value, &mut |out, inner_ind, tmp| {
                let n = next_tmp(tmp);
                line(
                    out,
                    inner_ind,
                    &format!("writer.WriteArrayHeader({value}.Count);"),
                );
                line(out, inner_ind, &format!("foreach (var __e{n} in {value})"));
                line(out, inner_ind, "{");
                line(out, inner_ind, &format!("    writer.Write(__e{n});"));
                line(out, inner_ind, "}");
                Ok(())
            })?;
        }
        UniDataType::Array(inner) => {
            if matches!(inner.as_ref(), UniDataType::Scalar(UniScalar::U8)) {
                // `list<u8>` maps to `byte[]`: MessagePack bin.
                nil_guard(&mut out, ind, tmp, value, &mut |out, inner_ind, _| {
                    line(out, inner_ind, &format!("writer.Write({value});"));
                    Ok(())
                })?;
            } else {
                nil_guard(&mut out, ind, tmp, value, &mut |out, inner_ind, tmp| {
                    let n = next_tmp(tmp);
                    line(
                        out,
                        inner_ind,
                        &format!("writer.WriteArrayHeader({value}.Count);"),
                    );
                    line(out, inner_ind, &format!("foreach (var __e{n} in {value})"));
                    line(out, inner_ind, "{");
                    let body_ind = format!("{inner_ind}    ");
                    out.push_str(&serialize_stmts(
                        inner,
                        &format!("__e{n}"),
                        &body_ind,
                        tmp,
                        type_kinds,
                    )?);
                    line(out, inner_ind, "}");
                    Ok(())
                })?;
            }
        }
        UniDataType::Option(inner) => {
            if csharp_is_reference_type_with_kinds(inner, type_kinds) {
                nil_guard(&mut out, ind, tmp, value, &mut |out, inner_ind, tmp| {
                    out.push_str(&serialize_stmts(inner, value, inner_ind, tmp, type_kinds)?);
                    Ok(())
                })?;
            } else {
                line(&mut out, ind, &format!("if ({value}.HasValue)"));
                line(&mut out, ind, "{");
                let body_ind = format!("{ind}    ");
                out.push_str(&serialize_stmts(
                    inner,
                    &format!("{value}.Value"),
                    &body_ind,
                    tmp,
                    type_kinds,
                )?);
                line(&mut out, ind, "}");
                line(&mut out, ind, "else");
                line(&mut out, ind, "{");
                line(&mut out, ind, "    writer.WriteNil();");
                line(&mut out, ind, "}");
            }
        }
        UniDataType::Identifier(name) => {
            let formatter = format!("{}Formatter", to_pascal_case(name));
            line(
                &mut out,
                ind,
                &format!("new {formatter}().Serialize(ref writer, {value}, options);"),
            );
        }
        UniDataType::Tuple(elems) => {
            line(
                &mut out,
                ind,
                &format!("writer.WriteArrayHeader({});", elems.len()),
            );
            for (i, elem) in elems.iter().enumerate() {
                out.push_str(&serialize_stmts(
                    elem,
                    &format!("{value}.Item{}", i + 1),
                    ind,
                    tmp,
                    type_kinds,
                )?);
            }
        }
        UniDataType::Box(inner) => {
            out.push_str(&serialize_stmts(inner, value, ind, tmp, type_kinds)?);
        }
        UniDataType::Record(_) | UniDataType::Result(_) => {
            serialize_fallback(&mut out, ind, value);
        }
    }
    Ok(out)
}

/// Emit C# statements reading a WIT `ty` value from `reader` into the
/// existing lvalue `target`. `required` mirrors the generated DTO's
/// `required` reference tracking: null-producing reads get the `!`
/// null-forgiving suffix exactly where the resolver-based emission had it.
pub fn deserialize_stmts(
    ty: &UniDataType,
    target: &str,
    required: bool,
    ind: &str,
    tmp: &mut usize,
) -> RS<String> {
    let mut out = String::new();
    match ty {
        UniDataType::Binary => {
            list_read_stmts(&mut out, ind, target, "byte", "reader.ReadByte()", tmp);
        }
        UniDataType::Array(inner)
            if !matches!(inner.as_ref(), UniDataType::Scalar(UniScalar::U8)) =>
        {
            let elem_ty = uni_data_type_to_name(inner, &LangKind::CSharp)?;
            let elem_expr = deserialize_expr(inner, false)
                .unwrap_or_else(|_| deserialize_fallback(&elem_ty, false));
            list_read_stmts(&mut out, ind, target, &elem_ty, &elem_expr, tmp);
        }
        UniDataType::Tuple(elems) => {
            let n = next_tmp(tmp);
            line(
                &mut out,
                ind,
                &format!("var __c{n} = reader.ReadArrayHeader();"),
            );
            line(&mut out, ind, &format!("if (__c{n} != {})", elems.len()));
            line(&mut out, ind, "{");
            line(
                &mut out,
                ind,
                &format!(
                    "    throw new global::System.InvalidOperationException($\"Expected array of length {}, got {{__c{n}}}\");",
                    elems.len()
                ),
            );
            line(&mut out, ind, "}");
            let mut names = Vec::with_capacity(elems.len());
            for (i, elem) in elems.iter().enumerate() {
                let elem_cs = uni_data_type_to_name(elem, &LangKind::CSharp)?;
                let local = format!("__t{n}_{i}");
                match deserialize_expr(elem, false) {
                    Ok(expr) => {
                        line(&mut out, ind, &format!("var {local} = {expr};"));
                    }
                    Err(_) => {
                        line(&mut out, ind, &format!("{elem_cs} {local};"));
                        out.push_str(&deserialize_stmts(elem, &local, false, ind, tmp)?);
                    }
                }
                names.push(local);
            }
            line(
                &mut out,
                ind,
                &format!("{target} = ({});", names.join(", ")),
            );
        }
        UniDataType::Option(inner) if !csharp_is_reference_type(inner) => {
            line(&mut out, ind, "if (reader.TryReadNil())");
            line(&mut out, ind, "{");
            line(&mut out, ind, &format!("    {target} = default;"));
            line(&mut out, ind, "}");
            line(&mut out, ind, "else");
            line(&mut out, ind, "{");
            let body_ind = format!("{ind}    ");
            out.push_str(&deserialize_stmts(inner, target, false, &body_ind, tmp)?);
            line(&mut out, ind, "}");
        }
        _ => {
            let expr = match deserialize_expr(ty, required) {
                Ok(expr) => expr,
                Err(_) => {
                    let cs_ty = uni_data_type_to_name(ty, &LangKind::CSharp)?;
                    deserialize_fallback(&cs_ty, required)
                }
            };
            line(&mut out, ind, &format!("{target} = {expr};"));
        }
    }
    Ok(out)
}

/// Emit C# statements reading a WIT `ty` value from `reader` into a fresh
/// local named `local` of C# type `cs_ty` (used for variant-case inners,
/// mirroring the historical `var xInner = Deserialize<T>(...)` shape).
pub fn deserialize_local(
    ty: &UniDataType,
    cs_ty: &str,
    local: &str,
    required: bool,
    ind: &str,
    tmp: &mut usize,
) -> RS<String> {
    match deserialize_expr(ty, required) {
        Ok(expr) => Ok(format!("{ind}var {local} = {expr};\n")),
        Err(_) => {
            let mut out = format!("{ind}{cs_ty} {local};\n");
            out.push_str(&deserialize_stmts(ty, local, required, ind, tmp)?);
            Ok(out)
        }
    }
}

/// Expression form of a resolver-free read for leaf shapes: scalars, strings,
/// `byte[]` blobs, user-defined types and options of reference leaves.
fn deserialize_expr(ty: &UniDataType, required: bool) -> RS<String> {
    let bang = if required { "!" } else { "" };
    let expr = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            // A bool crosses the Mudu procedure byte pipe as an i32 (0/1):
            // the host's uni-data-value vocabulary has no Bool case (the
            // desc declares the I32 family). Accept both the MessagePack
            // bool marker and the integer form (any width, exactly 0/1) so
            // record codecs decode host-supplied records unchanged.
            UniScalar::Bool => "(reader.NextMessagePackType == MessagePackType.Integer \
                 ? (reader.ReadInt64() switch { 0 => false, 1 => true, _ => throw new global::System.IO.InvalidDataException(\"MSSP expected a bool encoded as 0/1\") }) \
                 : reader.ReadBoolean())"
                .to_string(),
            UniScalar::U8 => "reader.ReadByte()".to_string(),
            UniScalar::I8 => "reader.ReadSByte()".to_string(),
            UniScalar::U16 => "reader.ReadUInt16()".to_string(),
            UniScalar::I16 => "reader.ReadInt16()".to_string(),
            UniScalar::U32 => "reader.ReadUInt32()".to_string(),
            UniScalar::I32 => "reader.ReadInt32()".to_string(),
            UniScalar::U64 => "reader.ReadUInt64()".to_string(),
            UniScalar::I64 => "reader.ReadInt64()".to_string(),
            UniScalar::F32 => "reader.ReadSingle()".to_string(),
            UniScalar::F64 => "reader.ReadDouble()".to_string(),
            // A WIT char is a single-character MessagePack string; nil or a
            // longer string is a protocol error. The library reads accept any
            // integer/float marker width (lenient decode), so the other
            // scalar reads need no marker dispatch of their own.
            UniScalar::Char => "(reader.ReadString() is { Length: 1 } __ch \
                 ? __ch[0] \
                 : throw new global::System.IO.InvalidDataException(\"MSSP expected a single-character string for a char value\"))"
                .to_string(),
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => format!("reader.ReadString(){bang}"),
            UniScalar::Blob => format!(
                "(reader.ReadBytes() is {{ }} __r ? global::System.Buffers.BuffersExtensions.ToArray(__r) : null){bang}"
            ),
            UniScalar::U128 | UniScalar::I128 => {
                let cs_ty = uni_data_type_to_name(ty, &LangKind::CSharp)?;
                deserialize_fallback(&cs_ty, required)
            }
        },
        // `list<u8>` maps to `byte[]`: MessagePack bin.
        UniDataType::Array(inner)
            if matches!(inner.as_ref(), UniDataType::Scalar(UniScalar::U8)) =>
        {
            format!(
                "(reader.ReadBytes() is {{ }} __r ? global::System.Buffers.BuffersExtensions.ToArray(__r) : null){bang}"
            )
        }
        UniDataType::Identifier(name) => {
            format!(
                "new {}Formatter().Deserialize(ref reader, options){bang}",
                to_pascal_case(name)
            )
        }
        // Reference-typed option arms read nil through the inner expression
        // (ReadString/ReadBytes/variant formatters all return null on nil).
        UniDataType::Option(inner) if csharp_is_reference_type(inner) => {
            deserialize_expr(inner, required)?
        }
        UniDataType::Box(inner) => deserialize_expr(inner, required)?,
        _ => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "no resolver-free C# read expression for this type"
            ));
        }
    };
    Ok(expr)
}

fn serialize_fallback(out: &mut String, ind: &str, value: &str) {
    line(
        out,
        ind,
        &format!("MessagePackSerializer.Serialize(ref writer, {value}, options);"),
    );
}

fn deserialize_fallback(cs_ty: &str, required: bool) -> String {
    let bang = if required { "!" } else { "" };
    format!("MessagePackSerializer.Deserialize<{cs_ty}>(ref reader, options){bang}")
}

/// `List<T>` read: nil maps to `null!` (the resolver's `ListFormatter`
/// behavior), otherwise an explicit header + element loop.
fn list_read_stmts(
    out: &mut String,
    ind: &str,
    target: &str,
    elem_ty: &str,
    elem_expr: &str,
    tmp: &mut usize,
) {
    let n = next_tmp(tmp);
    line(out, ind, "if (reader.TryReadNil())");
    line(out, ind, "{");
    line(out, ind, &format!("    {target} = null!;"));
    line(out, ind, "}");
    line(out, ind, "else");
    line(out, ind, "{");
    line(
        out,
        ind,
        &format!("    var __c{n} = reader.ReadArrayHeader();"),
    );
    line(
        out,
        ind,
        &format!("    var __l{n} = new List<{elem_ty}>(__c{n});"),
    );
    line(
        out,
        ind,
        &format!("    for (var __i{n} = 0; __i{n} < __c{n}; __i{n}++)"),
    );
    line(out, ind, "    {");
    line(out, ind, &format!("        __l{n}.Add({elem_expr});"));
    line(out, ind, "    }");
    line(out, ind, &format!("    {target} = __l{n};"));
    line(out, ind, "}");
}

/// Null guard: `if (v is null) { WriteNil } else { body }`, matching the
/// resolver formatters' nil encoding of null reference values.
fn nil_guard(
    out: &mut String,
    ind: &str,
    tmp: &mut usize,
    value: &str,
    body: &mut dyn FnMut(&mut String, &str, &mut usize) -> RS<()>,
) -> RS<()> {
    line(out, ind, &format!("if ({value} is null)"));
    line(out, ind, "{");
    line(out, ind, "    writer.WriteNil();");
    line(out, ind, "}");
    line(out, ind, "else");
    line(out, ind, "{");
    let inner_ind = format!("{ind}    ");
    body(out, &inner_ind, tmp)?;
    line(out, ind, "}");
    Ok(())
}

fn line(out: &mut String, ind: &str, text: &str) {
    out.push_str(ind);
    out.push_str(text);
    out.push('\n');
}

fn next_tmp(tmp: &mut usize) -> usize {
    let n = *tmp;
    *tmp += 1;
    n
}

#[cfg(test)]
mod tests {
    use super::{deserialize_local, deserialize_stmts, serialize_stmts};
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;
    use std::collections::HashMap;

    fn no_kinds() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn serialize_scalar_writes_raw_call() -> RS<()> {
        let mut tmp = 0;
        let s = serialize_stmts(
            &UniDataType::Scalar(UniScalar::U64),
            "value.L",
            "        ",
            &mut tmp,
            &no_kinds(),
        )?;
        assert_eq!(s, "        writer.Write(value.L);\n");
        Ok(())
    }

    #[test]
    fn serialize_string_nil_guards() -> RS<()> {
        let mut tmp = 0;
        let s = serialize_stmts(
            &UniDataType::Scalar(UniScalar::String),
            "value.ErrMsg",
            "        ",
            &mut tmp,
            &no_kinds(),
        )?;
        assert!(s.contains("if (value.ErrMsg is null)"));
        assert!(s.contains("writer.Write(value.ErrMsg);"));
        Ok(())
    }

    #[test]
    fn deserialize_binary_reads_byte_list() -> RS<()> {
        let mut tmp = 0;
        let s = deserialize_stmts(
            &UniDataType::Binary,
            "value.ErrDetails",
            true,
            "        ",
            &mut tmp,
        )?;
        assert!(s.contains("new List<byte>(__c0)"));
        assert!(s.contains("__l0.Add(reader.ReadByte());"));
        assert!(s.contains("value.ErrDetails = null!;"));
        Ok(())
    }

    #[test]
    fn deserialize_identifier_uses_formatter() -> RS<()> {
        let mut tmp = 0;
        let s = deserialize_local(
            &UniDataType::Identifier("uni-oid".to_string()),
            "UniOid",
            "oidInner",
            true,
            "                ",
            &mut tmp,
        )?;
        assert_eq!(
            s,
            "                var oidInner = new UniOidFormatter().Deserialize(ref reader, options)!;\n"
        );
        Ok(())
    }
}
