use crate::lang_impl;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_primitive::NonPrimitiveType;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_primitive::UniPrimitive;

pub fn uni_data_type_to_name(wit_ty: &UniDatType, lang: &LangKind) -> RS<String> {
    _to_lang_type(wit_ty, lang)
}

pub fn csharp_default_value_expr(wit_ty: &UniDatType) -> RS<String> {
    match wit_ty {
        UniDatType::Primitive(p_ty) => Ok(match p_ty {
            UniPrimitive::Bool => "false".to_string(),
            UniPrimitive::U8 => "0".to_string(),
            UniPrimitive::U16 => "0".to_string(),
            UniPrimitive::U32 => "0".to_string(),
            UniPrimitive::U64 => "0".to_string(),
            UniPrimitive::I8 => "0".to_string(),
            UniPrimitive::I16 => "0".to_string(),
            UniPrimitive::I32 => "0".to_string(),
            UniPrimitive::I64 => "0".to_string(),
            UniPrimitive::F32 => "0".to_string(),
            UniPrimitive::F64 => "0".to_string(),
            UniPrimitive::Char => "'\\0'".to_string(),
            UniPrimitive::String => "string.Empty".to_string(),
            UniPrimitive::Blob => "[]".to_string(),
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
        UniDatType::Primitive(p_ty) => matches!(p_ty, UniPrimitive::String | UniPrimitive::Blob),
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

fn to_primitive_type(wit_prim: &UniPrimitive, lang: &LangKind) -> RS<String> {
    Ok(lang_impl::lang_primitive_name(lang, wit_prim))
}

fn to_non_primitive_type(non_prim: &NonPrimitiveType, lang: &LangKind) -> RS<String> {
    Ok(lang_impl::lang_non_primitive_name(lang, non_prim))
}

fn handle_wit_tuple(vec_wit_ty: &Vec<UniDatType>, lang: &LangKind) -> RS<String> {
    let mut vec = Vec::new();
    for (_i, wit_ty) in vec_wit_ty.iter().enumerate() {
        let ty = uni_data_type_to_name(wit_ty, lang)?;
        vec.push(ty);
    }
    let non_prim = NonPrimitiveType::Tuple(vec);
    let s = to_non_primitive_type(&non_prim, lang)?;
    Ok(s)
}

fn _to_lang_type(wit_ty: &UniDatType, lang: &LangKind) -> RS<String> {
    let ty_str = match wit_ty {
        UniDatType::Primitive(p_ty) => {
            let s = to_primitive_type(p_ty, lang)?;
            s
        }
        UniDatType::Tuple(vec) => handle_wit_tuple(vec, lang)?,
        UniDatType::Array(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_prim = NonPrimitiveType::Array(inner);
            to_non_primitive_type(&non_prim, lang)?
        }
        UniDatType::Option(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_prim = NonPrimitiveType::Option(inner);
            to_non_primitive_type(&non_prim, lang)?
        }
        UniDatType::Identifier(ty_name) => to_pascal_case(ty_name),
        UniDatType::Box(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_prim = NonPrimitiveType::Box(inner);
            to_non_primitive_type(&non_prim, lang)?
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
