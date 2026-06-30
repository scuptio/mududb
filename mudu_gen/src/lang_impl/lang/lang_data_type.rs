//! Convert universal data types into language-specific type names.

use crate::lang_impl;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Convert a [`UniDatType`] into a language-specific type name.
pub fn uni_data_type_to_name(wit_ty: &UniDatType, lang: &LangKind) -> RS<String> {
    _to_lang_type(wit_ty, lang)
}

/// Return the C# default-value expression for a [`UniDatType`].
pub fn csharp_default_value_expr(wit_ty: &UniDatType) -> RS<String> {
    match wit_ty {
        UniDatType::Scalar(p_ty) => Ok(match p_ty {
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
        UniDatType::Tuple(_) => Ok("default".to_string()),
        UniDatType::Array(_) => Ok("[]".to_string()),
        UniDatType::Option(inner_ty) => csharp_default_value_expr(inner_ty),
        UniDatType::Identifier(ty_name) => Ok(format!("new {}()", to_pascal_case(ty_name))),
        UniDatType::Box(inner_ty) => csharp_default_value_expr(inner_ty),
        UniDatType::Result { .. } => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "C# default value for result type is not implemented"
        )),
        UniDatType::Record { .. } => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "C# default value for record type is not implemented"
        )),
        UniDatType::Binary => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "C# default value for binary type is not implemented"
        )),
    }
}

/// Return whether a [`UniDatType`] is a C# reference type.
pub fn csharp_is_reference_type(wit_ty: &UniDatType) -> bool {
    match wit_ty {
        UniDatType::Scalar(p_ty) => {
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
        UniDatType::Tuple(_) => false,
        UniDatType::Array(_) => true,
        UniDatType::Option(inner_ty) => csharp_is_reference_type(inner_ty),
        UniDatType::Identifier(_) => true,
        UniDatType::Box(inner_ty) => csharp_is_reference_type(inner_ty),
        UniDatType::Result { .. } => true,
        UniDatType::Record { .. } => true,
        UniDatType::Binary => true,
    }
}

fn to_scalar_type(wit_prim: &UniScalar, lang: &LangKind) -> RS<String> {
    Ok(lang_impl::lang_scalar_name(lang, wit_prim))
}

fn to_non_scalar_type(non_scalar: &NonScalarType, lang: &LangKind) -> RS<String> {
    Ok(lang_impl::lang_non_scalar_name(lang, non_scalar))
}

fn handle_wit_tuple(vec_wit_ty: &[UniDatType], lang: &LangKind) -> RS<String> {
    let mut vec = Vec::new();
    for wit_ty in vec_wit_ty.iter() {
        let ty = uni_data_type_to_name(wit_ty, lang)?;
        vec.push(ty);
    }
    let non_scalar = NonScalarType::Tuple(vec);
    let s = to_non_scalar_type(&non_scalar, lang)?;
    Ok(s)
}

fn _to_lang_type(wit_ty: &UniDatType, lang: &LangKind) -> RS<String> {
    let ty_str = match wit_ty {
        UniDatType::Scalar(p_ty) => to_scalar_type(p_ty, lang)?,
        UniDatType::Tuple(vec) => handle_wit_tuple(vec, lang)?,
        UniDatType::Array(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_scalar = NonScalarType::Array(inner);
            to_non_scalar_type(&non_scalar, lang)?
        }
        UniDatType::Option(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_scalar = NonScalarType::Option(inner);
            to_non_scalar_type(&non_scalar, lang)?
        }
        UniDatType::Identifier(ty_name) => to_pascal_case(ty_name),
        UniDatType::Box(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_scalar = NonScalarType::Box(inner);
            to_non_scalar_type(&non_scalar, lang)?
        }
        UniDatType::Result { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "result type is not implemented for language code generation"
            ));
        }
        UniDatType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "record type is not implemented for language code generation"
            ));
        }
        UniDatType::Binary => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "binary type is not implemented for language code generation"
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
    use mudu_binding::universal::uni_dat_type::UniDatType;
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
            uni_data_type_to_name(&UniDatType::Scalar(UniScalar::I32), &LangKind::Rust)?,
            "i32"
        );
        assert_eq!(
            uni_data_type_to_name(&UniDatType::Scalar(UniScalar::I32), &LangKind::CSharp)?,
            "int"
        );
        Ok(())
    }

    #[test]
    fn composite_types_map_to_language_names() -> RS<()> {
        let array = UniDatType::Array(Box::new(UniDatType::Scalar(UniScalar::String)));
        assert_eq!(
            uni_data_type_to_name(&array, &LangKind::Rust)?,
            "Vec<String>"
        );
        assert_eq!(
            uni_data_type_to_name(&array, &LangKind::CSharp)?,
            "List<string>"
        );

        let opt = UniDatType::Option(Box::new(UniDatType::Scalar(UniScalar::I64)));
        assert_eq!(uni_data_type_to_name(&opt, &LangKind::Rust)?, "Option<i64>");
        assert_eq!(uni_data_type_to_name(&opt, &LangKind::CSharp)?, "long");

        let tuple = UniDatType::Tuple(vec![
            UniDatType::Scalar(UniScalar::I32),
            UniDatType::Scalar(UniScalar::String),
        ]);
        assert!(uni_data_type_to_name(&tuple, &LangKind::Rust)?.contains("i32"));
        Ok(())
    }

    #[test]
    fn unsupported_types_return_not_implemented() -> RS<()> {
        let result = UniDatType::Result(mudu_binding::universal::uni_result_type::UniResultType {
            ok: None,
            err: None,
        });
        assert_not_implemented(uni_data_type_to_name(&result, &LangKind::Rust))?;

        let binary = UniDatType::Binary;
        assert_not_implemented(uni_data_type_to_name(&binary, &LangKind::CSharp))?;
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_scalars() -> RS<()> {
        assert_eq!(
            csharp_default_value_expr(&UniDatType::Scalar(UniScalar::Bool))?,
            "false"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDatType::Scalar(UniScalar::I32))?,
            "0"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDatType::Scalar(UniScalar::String))?,
            "string.Empty"
        );
        assert_eq!(
            csharp_default_value_expr(&UniDatType::Scalar(UniScalar::Blob))?,
            "[]"
        );
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_composites() -> RS<()> {
        let arr = UniDatType::Array(Box::new(UniDatType::Scalar(UniScalar::I32)));
        assert_eq!(csharp_default_value_expr(&arr)?, "[]");

        let id = UniDatType::Identifier("my_type".to_string());
        assert_eq!(csharp_default_value_expr(&id)?, "new MyType()");

        let opt = UniDatType::Option(Box::new(UniDatType::Scalar(UniScalar::String)));
        assert_eq!(csharp_default_value_expr(&opt)?, "string.Empty");
        Ok(())
    }

    #[test]
    fn csharp_default_value_expr_unsupported() -> RS<()> {
        let result = UniDatType::Result(mudu_binding::universal::uni_result_type::UniResultType {
            ok: None,
            err: None,
        });
        assert_not_implemented(csharp_default_value_expr(&result))?;
        Ok(())
    }

    #[test]
    fn csharp_is_reference_type_detects_reference_types() {
        assert!(csharp_is_reference_type(&UniDatType::Scalar(
            UniScalar::String
        )));
        assert!(csharp_is_reference_type(&UniDatType::Array(Box::new(
            UniDatType::Scalar(UniScalar::I32)
        ))));
        assert!(csharp_is_reference_type(&UniDatType::Identifier(
            "t".to_string()
        )));
        assert!(!csharp_is_reference_type(&UniDatType::Scalar(
            UniScalar::I32
        )));
        assert!(!csharp_is_reference_type(&UniDatType::Tuple(vec![])));
    }
}
