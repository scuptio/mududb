//! Convert universal data types into language-specific type names.

use crate::lang_impl;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use std::collections::HashMap;

/// Convert a [`UniDataType`] into a language-specific type name.
pub fn uni_data_type_to_name(wit_ty: &UniDataType, lang: &LangKind) -> RS<String> {
    _to_lang_type(wit_ty, lang)
}

/// Return the C# default-value expression for a [`UniDataType`].
///
/// `type_kinds` resolves user-defined identifiers: variant (interface) types
/// are not constructable, so their default is the formatter's
/// `DefaultInstance()` (the first declared case with its default payload —
/// the proto3-style default of a missing variant-typed field); records and
/// enums use `new X()`. Unknown (unregistered) identifiers fall back to
/// `new X()`.
pub fn csharp_default_value_expr(
    wit_ty: &UniDataType,
    type_kinds: &HashMap<String, String>,
) -> RS<String> {
    match wit_ty {
        UniDataType::Scalar(p_ty) => Ok(match p_ty {
            UniScalar::Bool => "false".to_string(),
            UniScalar::U8 => "0".to_string(),
            UniScalar::U16 => "0".to_string(),
            UniScalar::U32 => "0".to_string(),
            UniScalar::U64 => "0".to_string(),
            UniScalar::U128 => "default".to_string(),
            UniScalar::I8 => "0".to_string(),
            UniScalar::I16 => "0".to_string(),
            UniScalar::I32 => "0".to_string(),
            UniScalar::I64 => "0".to_string(),
            UniScalar::I128 => "0".to_string(),
            UniScalar::F32 => "0".to_string(),
            UniScalar::F64 => "0".to_string(),
            UniScalar::Char => "'\\0'".to_string(),
            UniScalar::String => "string.Empty".to_string(),
            UniScalar::Blob => "[]".to_string(),
            UniScalar::Numeric => "string.Empty".to_string(),
            UniScalar::Date => "string.Empty".to_string(),
            UniScalar::Time => "string.Empty".to_string(),
            UniScalar::Timestamp => "string.Empty".to_string(),
            UniScalar::TimestampTz => "string.Empty".to_string(),
        }),
        UniDataType::Tuple(_) => Ok("default".to_string()),
        UniDataType::Array(_) => Ok("[]".to_string()),
        UniDataType::Option(_) => Ok("default".to_string()),
        UniDataType::Box(inner_ty) => csharp_default_value_expr(inner_ty, type_kinds),
        UniDataType::Binary => Ok("[]".to_string()),
        UniDataType::Identifier(ty_name) => {
            let pascal = to_pascal_case(ty_name);
            match type_kinds.get(&pascal).map(|k| k.as_str()) {
                Some("variant") => Ok(format!("{}Formatter.DefaultInstance()", pascal)),
                _ => Ok(format!("new {}()", pascal)),
            }
        }
        UniDataType::Result { .. } => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "C# default value for result type is not implemented"
        )),
        UniDataType::Record { .. } => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "C# default value for record type is not implemented"
        )),
    }
}

/// Return the AssemblyScript default-value expression for a [`UniDataType`].
pub fn assemblyscript_default_value_expr(wit_ty: &UniDataType) -> RS<String> {
    match wit_ty {
        UniDataType::Scalar(p_ty) => Ok(match p_ty {
            UniScalar::Bool => "false".to_string(),
            UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::U128
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64
            | UniScalar::I128
            | UniScalar::F32
            | UniScalar::F64 => "0".to_string(),
            UniScalar::Char | UniScalar::String => "\"\"".to_string(),
            UniScalar::Blob => "new Uint8Array(0)".to_string(),
            UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => "\"\"".to_string(),
        }),
        UniDataType::Tuple(_) => Ok("[]".to_string()),
        UniDataType::Array(_) => Ok("[]".to_string()),
        UniDataType::Option(_) => Ok("null".to_string()),
        UniDataType::Box(inner_ty) => assemblyscript_default_value_expr(inner_ty),
        UniDataType::Binary => Ok("new Uint8Array(0)".to_string()),
        UniDataType::Identifier(ty_name) => Ok(format!("new {}()", to_pascal_case(ty_name))),
        UniDataType::Result { .. } => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "AssemblyScript default value for result type is not implemented"
        )),
        UniDataType::Record { .. } => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "AssemblyScript default value for record type is not implemented"
        )),
    }
}

/// Return whether a [`UniDataType`] is a C# reference type.
pub fn csharp_is_reference_type(wit_ty: &UniDataType) -> bool {
    match wit_ty {
        UniDataType::Scalar(p_ty) => {
            matches!(
                p_ty,
                UniScalar::String
                    | UniScalar::Blob
                    | UniScalar::Numeric
                    | UniScalar::Date
                    | UniScalar::Time
                    | UniScalar::Timestamp
                    | UniScalar::TimestampTz
            )
        }
        UniDataType::Tuple(_) => false,
        UniDataType::Array(_) => true,
        UniDataType::Option(inner_ty) => csharp_is_reference_type(inner_ty),
        UniDataType::Box(inner_ty) => csharp_is_reference_type(inner_ty),
        UniDataType::Binary => true,
        UniDataType::Identifier(_) => true,
        UniDataType::Result { .. } => true,
        UniDataType::Record { .. } => true,
    }
}

/// Reference-type check that resolves named WIT types through the
/// `type_kinds` registry: records and enums generate C# structs (value
/// types), only variants generate interfaces (reference types). Unregistered
/// names keep the historical reference-type assumption.
pub fn csharp_is_reference_type_with_kinds(
    wit_ty: &UniDataType,
    type_kinds: &HashMap<String, String>,
) -> bool {
    match wit_ty {
        UniDataType::Identifier(name) => type_kinds
            .get(&to_pascal_case(name))
            .map(|kind| kind != "record" && kind != "enum")
            .unwrap_or(true),
        other => csharp_is_reference_type(other),
    }
}

fn to_scalar_type(wit_prim: &UniScalar, lang: &LangKind) -> RS<String> {
    Ok(lang_impl::lang_scalar_name(lang, wit_prim))
}

/// The unit type name used for absent `result` arms.
fn unit_type_name(lang: &LangKind) -> String {
    match lang {
        LangKind::Rust => "()".to_string(),
        LangKind::CSharp => "byte".to_string(),
        LangKind::AssemblyScript => "bool".to_string(),
        LangKind::Python => "None".to_string(),
        LangKind::C => "void".to_string(),
        LangKind::Go => "struct{}".to_string(),
    }
}

fn to_non_scalar_type(non_scalar: &NonScalarType, lang: &LangKind) -> RS<String> {
    Ok(lang_impl::lang_non_scalar_name(lang, non_scalar))
}

fn handle_wit_tuple(vec_wit_ty: &[UniDataType], lang: &LangKind) -> RS<String> {
    let mut vec = Vec::new();
    for wit_ty in vec_wit_ty.iter() {
        let ty = uni_data_type_to_name(wit_ty, lang)?;
        vec.push(ty);
    }
    let non_scalar = NonScalarType::Tuple(vec);
    let s = to_non_scalar_type(&non_scalar, lang)?;
    Ok(s)
}

fn _to_lang_type(wit_ty: &UniDataType, lang: &LangKind) -> RS<String> {
    let ty_str = match wit_ty {
        UniDataType::Scalar(p_ty) => to_scalar_type(p_ty, lang)?,
        UniDataType::Tuple(vec) => handle_wit_tuple(vec, lang)?,
        UniDataType::Array(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_scalar = NonScalarType::Array(inner);
            to_non_scalar_type(&non_scalar, lang)?
        }
        UniDataType::Option(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            if *lang == LangKind::CSharp {
                format!("{}?", inner)
            } else {
                let non_scalar = NonScalarType::Option(inner);
                to_non_scalar_type(&non_scalar, lang)?
            }
        }
        UniDataType::Identifier(ty_name) => to_pascal_case(ty_name),
        UniDataType::Box(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_scalar = NonScalarType::Box(inner);
            to_non_scalar_type(&non_scalar, lang)?
        }
        UniDataType::Binary => match lang {
            // Record-context `list<u8>`: a plain MessagePack ARRAY of u8 (the
            // Rust host encodes `Vec<u8>` record fields as arrays, not bin).
            // `List<byte>` matches that shape with MessagePack-CSharp;
            // `byte[]` would encode as bin. Func-level `list<u8>` blobs are
            // `byte[]` (bin) and are mapped by the func templates instead.
            LangKind::CSharp => "List<byte>".to_string(),
            _ => to_scalar_type(&UniScalar::Blob, lang)?,
        },
        UniDataType::Result(result_ty) => {
            let ok_name = match &result_ty.ok {
                Some(ok_ty) => uni_data_type_to_name(ok_ty, lang)?,
                None => unit_type_name(lang),
            };
            match lang {
                LangKind::Rust => {
                    let err_name = match &result_ty.err {
                        Some(err_ty) => uni_data_type_to_name(err_ty, lang)?,
                        None => unit_type_name(lang),
                    };
                    format!("Result<{}, {}>", ok_name, err_name)
                }
                LangKind::CSharp => {
                    // Mirrors the generated C# decode signatures: a value
                    // result decodes to `SyscallResult<T>`, a unit result to
                    // the carried `UniError?` (null on success).
                    if result_ty.ok.is_some() {
                        format!("SyscallResult<{}>", ok_name)
                    } else {
                        "UniError?".to_string()
                    }
                }
                LangKind::AssemblyScript => {
                    // The func template generates per-function result holder
                    // classes; the generic mapping falls back to the ok type
                    // name (`bool` placeholder for unit results).
                    ok_name
                }
                LangKind::Python => {
                    // Python funcs return the untyped `WireResult` holder; the
                    // generic mapping falls back to the ok type name.
                    ok_name
                }
                LangKind::C | LangKind::Go => {
                    // C/Go generation covers entities only; fall back to the
                    // ok type name for the (unsupported) func result mapping.
                    ok_name
                }
            }
        }
        UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "record type is not implemented for language code generation"
            ));
        }
    };
    Ok(ty_str)
}

#[cfg(test)]
mod tests {
    use super::{csharp_default_value_expr, csharp_is_reference_type, uni_data_type_to_name};
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    fn assert_not_implemented(result: RS<String>) -> RS<()> {
        match result {
            Ok(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "expected a NotImplemented error"
            )),
            Err(err) => {
                assert_eq!(err.ec(), ErrorCode::NotImplemented);
                Ok(())
            }
        }
    }

    #[test]
    fn scalar_types_map_to_language_names() -> RS<()> {
        assert_eq!(
            uni_data_type_to_name(&UniDataType::Scalar(UniScalar::I32), &LangKind::Rust)?,
            "i32"
        );
        assert_eq!(
            uni_data_type_to_name(&UniDataType::Scalar(UniScalar::I32), &LangKind::CSharp)?,
            "int"
        );
        Ok(())
    }

    #[test]
    fn composite_types_map_to_language_names() -> RS<()> {
        let array = UniDataType::Array(Box::new(UniDataType::Scalar(UniScalar::String)));
        assert_eq!(
            uni_data_type_to_name(&array, &LangKind::Rust)?,
            "Vec<String>"
        );
        assert_eq!(
            uni_data_type_to_name(&array, &LangKind::CSharp)?,
            "List<string>"
        );

        let opt = UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::I64)));
        assert_eq!(uni_data_type_to_name(&opt, &LangKind::Rust)?, "Option<i64>");
        assert_eq!(uni_data_type_to_name(&opt, &LangKind::CSharp)?, "long?");

        let tuple = UniDataType::Tuple(vec![
            UniDataType::Scalar(UniScalar::I32),
            UniDataType::Scalar(UniScalar::String),
        ]);
        assert!(uni_data_type_to_name(&tuple, &LangKind::Rust)?.contains("i32"));
        Ok(())
    }

    #[test]
    fn result_type_maps_to_language_names() -> RS<()> {
        let result = UniDataType::Result(mudu_binding::universal::uni_result_type::UniResultType {
            ok: Some(Box::new(UniDataType::Scalar(UniScalar::U32))),
            err: Some(Box::new(UniDataType::Identifier("uni-error".to_string()))),
        });
        assert_eq!(
            uni_data_type_to_name(&result, &LangKind::Rust)?,
            "Result<u32, UniError>"
        );
        assert_eq!(
            uni_data_type_to_name(&result, &LangKind::CSharp)?,
            "SyscallResult<uint>"
        );
        assert_eq!(
            uni_data_type_to_name(&result, &LangKind::AssemblyScript)?,
            "u32"
        );

        let unit_result =
            UniDataType::Result(mudu_binding::universal::uni_result_type::UniResultType {
                ok: None,
                err: Some(Box::new(UniDataType::Identifier("uni-error".to_string()))),
            });
        assert_eq!(
            uni_data_type_to_name(&unit_result, &LangKind::Rust)?,
            "Result<(), UniError>"
        );
        assert_eq!(
            uni_data_type_to_name(&unit_result, &LangKind::CSharp)?,
            "UniError?"
        );
        Ok(())
    }

    #[test]
    fn binary_and_box_types_map_to_language_names() -> RS<()> {
        let binary = UniDataType::Binary;
        assert_eq!(uni_data_type_to_name(&binary, &LangKind::Rust)?, "Vec<u8>");
        // Record-context `list<u8>` is a MessagePack array, hence `List<byte>`.
        assert_eq!(
            uni_data_type_to_name(&binary, &LangKind::CSharp)?,
            "List<byte>"
        );
        assert_eq!(
            uni_data_type_to_name(&binary, &LangKind::AssemblyScript)?,
            "Uint8Array"
        );

        let boxed = UniDataType::Box(Box::new(UniDataType::Scalar(UniScalar::I32)));
        assert_eq!(uni_data_type_to_name(&boxed, &LangKind::Rust)?, "Box<i32>");
        assert_eq!(uni_data_type_to_name(&boxed, &LangKind::CSharp)?, "int");
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_scalars() -> RS<()> {
        assert_eq!(
            csharp_default_value_expr(&UniDataType::Scalar(UniScalar::Bool), &Default::default())?,
            "false"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDataType::Scalar(UniScalar::I32), &Default::default())?,
            "0"
        );
        assert_eq!(
            csharp_default_value_expr(
                &UniDataType::Scalar(UniScalar::String),
                &Default::default()
            )?,
            "string.Empty"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDataType::Scalar(UniScalar::Blob), &Default::default())?,
            "[]"
        );
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_composites() -> RS<()> {
        let arr = UniDataType::Array(Box::new(UniDataType::Scalar(UniScalar::I32)));
        assert_eq!(csharp_default_value_expr(&arr, &Default::default())?, "[]");

        let id = UniDataType::Identifier("my_type".to_string());
        assert_eq!(
            csharp_default_value_expr(&id, &Default::default())?,
            "new MyType()"
        );

        let opt = UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::String)));
        assert_eq!(
            csharp_default_value_expr(&opt, &Default::default())?,
            "default"
        );
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_unsupported() -> RS<()> {
        let result = UniDataType::Result(mudu_binding::universal::uni_result_type::UniResultType {
            ok: None,
            err: None,
        });
        assert_not_implemented(csharp_default_value_expr(&result, &Default::default()))?;
        Ok(())
    }

    #[test]
    fn csharp_is_reference_type_detects_reference_types() {
        assert!(csharp_is_reference_type(&UniDataType::Scalar(
            UniScalar::String
        )));
        assert!(csharp_is_reference_type(&UniDataType::Array(Box::new(
            UniDataType::Scalar(UniScalar::I32)
        ))));
        assert!(csharp_is_reference_type(&UniDataType::Identifier(
            "t".to_string()
        )));
        assert!(!csharp_is_reference_type(&UniDataType::Scalar(
            UniScalar::I32
        )));
        assert!(!csharp_is_reference_type(&UniDataType::Tuple(vec![])));
    }
}
