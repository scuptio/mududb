use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniEnumDef;

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub enum_comments: String,
    pub enum_name: String,
    pub enum_cases: Vec<EnumCaseInfo>,
}

#[derive(Debug, Clone)]
pub struct EnumCaseInfo {
    pub ec_comments: String,
    pub ec_name: String,
    pub ec_number: u32,
}

impl EnumInfo {
    pub fn from(enum_def: UniEnumDef, _lang: LangKind) -> RS<Self> {
        let name = to_pascal_case(&enum_def.enum_name);
        let mut cases = Vec::new();
        for (_i, v) in enum_def.enum_cases.into_iter().enumerate() {
            let ec_info = EnumCaseInfo {
                ec_name: to_pascal_case(&v.ec_name),
                ec_comments: v.ec_comments,
                ec_number: v.ec_number,
            };
            cases.push(ec_info);
        }

        let enum_def = EnumInfo {
            enum_comments: enum_def.enum_comments,
            enum_name: name,
            enum_cases: cases,
        };
        Ok(enum_def)
    }
}