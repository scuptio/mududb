use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_def::UniVariantDef;
use mudu_binding::universal::uni_primitive::UniPrimitive;

#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub variant_comments: String,
    pub variant_name: String,
    pub variant_cases: Vec<VariantCaseInfo>,
}

#[derive(Debug, Clone)]
pub struct VariantCaseInfo {
    pub vc_number: u32,
    pub vc_comments: String,
    pub vc_case_name: String,
    pub vc_case_name_snake:String,
    pub vc_has_inner_type:bool,
    pub vc_inner_type_name: String,
}


impl VariantInfo {
    pub fn from(variant_def: UniVariantDef, lang: LangKind) -> RS<Self> {
        let name = to_pascal_case(&variant_def.variant_name);
        let mut variants = Vec::with_capacity(variant_def.variant_cases.len());
        for (i, v) in variant_def.variant_cases.into_iter().enumerate() {
            let (vc_has_inner_type, vc_inner_type_name) = match v.vc_case_type {
                Some(ty) => {
                    (true, uni_data_type_to_name(&ty, &lang)?)
                }
                None => {
                    (false, uni_data_type_to_name(&UniDatType::Primitive(UniPrimitive::U8), &lang)?)
                }
            };
            let vc = VariantCaseInfo {
                vc_number: i as _,
                vc_comments: v.vc_comments,
                vc_case_name:to_pascal_case(&v.vc_case_name),
                vc_case_name_snake:to_snake_case(&v.vc_case_name),
                vc_has_inner_type,
                vc_inner_type_name,
            };
            variants.push(vc)
        }
        let variant = VariantInfo {
            variant_comments:variant_def.variant_comments,
            variant_name: name,
            variant_cases: variants,
        };
        Ok(variant)
    }
}