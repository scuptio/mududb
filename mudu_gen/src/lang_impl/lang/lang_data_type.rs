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
            UniDatType::Tuple(vec) => {
            handle_wit_tuple(vec, lang)?
        }
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
            UniDatType::Identifier(ty_name) => {
            to_pascal_case(ty_name)
        }
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
