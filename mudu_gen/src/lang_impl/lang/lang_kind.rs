use mudu::common::result::RS;
use mudu_binding::universal::uni_primitive::UniPrimitive;

use crate::lang_impl;
use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use mudu_binding::universal::uni_dat_type::UniDatType;

#[derive(Debug, PartialOrd, PartialEq, Eq, Copy, Clone)]
pub enum LangKind {
    Rust,
    CSharp,
}


impl LangKind {
    pub fn to_str(&self) -> &'static str {
        match self {
            LangKind::Rust => "rust",
            LangKind::CSharp => "csharp",
        }
    }
    pub fn from_str(lang: &str) -> Option<LangKind> {
        let s = lang.to_lowercase();
        match s.as_str() {
            "rust" => Some(LangKind::Rust),
            "csharp" => Some(LangKind::CSharp),
            _ => None
        }
    }

    pub fn name_of_primitive(&self, p: &UniPrimitive) -> RS<String> {
        Ok(lang_impl::lang_primitive_name(self, p))
    }

    pub fn name_of_wit_type(&self, wit_type: &UniDatType) -> RS<String> {
        uni_data_type_to_name(wit_type, self)
    }

    pub fn extension(&self) -> &'static str {
        match self {
            LangKind::Rust => "rs",
            LangKind::CSharp => "cs",
        }
    }
}
