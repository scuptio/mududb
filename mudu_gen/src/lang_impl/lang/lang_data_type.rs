use crate::lang_impl;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_scalar::UniScalar;

pub fn uni_data_type_to_name(wit_ty: &UniDatType, lang: &LangKind) -> RS<String> {
    _to_lang_type(wit_ty, lang)
}

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
        UniDatType::Result { .. } => {
            unimplemented!()
        }
        UniDatType::Record { .. } => {
            unimplemented!()
        }
        UniDatType::Binary => {
            unimplemented!()
        }
    }
}

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

fn handle_wit_tuple(vec_wit_ty: &Vec<UniDatType>, lang: &LangKind) -> RS<String> {
    let mut vec = Vec::new();
    for (_i, wit_ty) in vec_wit_ty.iter().enumerate() {
        let ty = uni_data_type_to_name(wit_ty, lang)?;
        vec.push(ty);
    }
    let non_scalar = NonScalarType::Tuple(vec);
    let s = to_non_scalar_type(&non_scalar, lang)?;
    Ok(s)
}

fn _to_lang_type(wit_ty: &UniDatType, lang: &LangKind) -> RS<String> {
    let ty_str = match wit_ty {
        UniDatType::Scalar(p_ty) => {
            let s = to_scalar_type(p_ty, lang)?;
            s
        }
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
            unimplemented!()
        }
        UniDatType::Record { .. } => {
            unimplemented!()
        }
        UniDatType::Binary => {
            unimplemented!()
        }
    };
    Ok(ty_str)
}
