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

/// Convert a [`UniDataType`] into a language-specific type name.
pub fn uni_data_type_to_name(wit_ty: &UniDataType, lang: &LangKind) -> RS<String> {
    _to_lang_type(wit_ty, lang)
}

/// Return the C# default-value expression for a [`UniDataType`].
pub fn csharp_default_value_expr(wit_ty: &UniDataType) -> RS<String> {
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
        UniDataType::Box(inner_ty) => csharp_default_value_expr(inner_ty),
        UniDataType::Binary => Ok("[]".to_string()),
        UniDataType::Identifier(ty_name) => Ok(format!("new {}()", to_pascal_case(ty_name))),
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

fn to_scalar_type(wit_prim: &UniScalar, lang: &LangKind) -> RS<String> {
    Ok(lang_impl::lang_scalar_name(lang, wit_prim))
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
        UniDataType::Binary => to_scalar_type(&UniScalar::Blob, lang)?,
        UniDataType::Result { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "result type is not implemented for language code generation"
            ));
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
    fn unsupported_types_return_not_implemented() -> RS<()> {
        let result = UniDataType::Result(mudu_binding::universal::uni_result_type::UniResultType {
            ok: None,
            err: None,
        });
        assert_not_implemented(uni_data_type_to_name(&result, &LangKind::Rust))?;
        Ok(())
    }

    #[test]
    fn binary_and_box_types_map_to_language_names() -> RS<()> {
        let binary = UniDataType::Binary;
        assert_eq!(uni_data_type_to_name(&binary, &LangKind::Rust)?, "Vec<u8>");
        assert_eq!(uni_data_type_to_name(&binary, &LangKind::CSharp)?, "byte[]");

        let boxed = UniDataType::Box(Box::new(UniDataType::Scalar(UniScalar::I32)));
        assert_eq!(uni_data_type_to_name(&boxed, &LangKind::Rust)?, "Box<i32>");
        assert_eq!(uni_data_type_to_name(&boxed, &LangKind::CSharp)?, "int");
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_scalars() -> RS<()> {
        assert_eq!(
            csharp_default_value_expr(&UniDataType::Scalar(UniScalar::Bool))?,
            "false"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDataType::Scalar(UniScalar::I32))?,
            "0"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDataType::Scalar(UniScalar::String))?,
            "string.Empty"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDataType::Scalar(UniScalar::Blob))?,
            "[]"
        );
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_composites() -> RS<()> {
        let arr = UniDataType::Array(Box::new(UniDataType::Scalar(UniScalar::I32)));
        assert_eq!(csharp_default_value_expr(&arr)?, "[]");

        let id = UniDataType::Identifier("my_type".to_string());
        assert_eq!(csharp_default_value_expr(&id)?, "new MyType()");

        let opt = UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::String)));
        assert_eq!(csharp_default_value_expr(&opt)?, "default");
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_unsupported() -> RS<()> {
        let result = UniDataType::Result(mudu_binding::universal::uni_result_type::UniResultType {
            ok: None,
            err: None,
        });
        assert_not_implemented(csharp_default_value_expr(&result))?;
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
