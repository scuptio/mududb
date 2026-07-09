use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniEnumDef;

/// Language-normalized enum metadata.
#[derive(Debug, Clone)]
pub struct EnumInfo {
    /// Enum doc comments.
    pub enum_comments: String,
    /// Pascal-case enum name.
    pub enum_name: String,
    /// Normalized enum cases.
    pub enum_cases: Vec<EnumCaseInfo>,
}

/// Language-normalized enum case metadata.
#[derive(Debug, Clone)]
pub struct EnumCaseInfo {
    /// Case doc comments.
    pub ec_comments: String,
    /// Pascal-case case name.
    pub ec_name: String,
    /// Numeric discriminator.
    pub ec_number: u32,
}

impl EnumInfo {
    /// Convert a [`UniEnumDef`] into an [`EnumInfo`] for the target language.
    pub fn from(enum_def: UniEnumDef, _lang: LangKind) -> RS<Self> {
        let name = to_pascal_case(&enum_def.enum_name);
        let mut cases = Vec::new();
        for v in enum_def.enum_cases.into_iter() {
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

#[cfg(test)]
mod tests {
    use super::EnumInfo;
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_def::{EnumCase, UniEnumDef};

    #[test]
    fn from_normalizes_enum_metadata() -> RS<()> {
        let enum_def = UniEnumDef {
            enum_comments: "comment".to_string(),
            enum_name: "mu-type-family".to_string(),
            enum_cases: vec![
                EnumCase {
                    ec_comments: "i32".to_string(),
                    ec_name: "i32".to_string(),
                    ec_number: 0,
                },
                EnumCase {
                    ec_comments: "i64".to_string(),
                    ec_name: "i64".to_string(),
                    ec_number: 1,
                },
            ],
        };
        let info = EnumInfo::from(enum_def, LangKind::Rust)?;
        assert_eq!(info.enum_name, "MuTypeFamily");
        assert_eq!(info.enum_cases.len(), 2);
        assert_eq!(info.enum_cases[0].ec_name, "I32");
        assert_eq!(info.enum_cases[1].ec_number, 1);
        Ok(())
    }
}
