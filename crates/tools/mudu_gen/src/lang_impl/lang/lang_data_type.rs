//! Convert universal data types into language-specific type names.

pub use crate::lang_impl::lang::name::uni_data_type_to_name;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use std::collections::HashMap;

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
        UniDataType::Struct { .. } => Err(mudu_error!(
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
        UniDataType::Struct { .. } => Err(mudu_error!(
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
        UniDataType::Struct { .. } => true,
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
        // Struct-context `list<u8>` is a MessagePack array, hence `List<byte>`.
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
