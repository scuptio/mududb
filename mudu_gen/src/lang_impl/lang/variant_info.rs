use crate::lang_impl::lang::lang_data_type::{
    csharp_default_value_expr, csharp_is_reference_type, uni_data_type_to_name,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_def::UniVariantDef;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Language-normalized variant metadata.
#[derive(Debug, Clone)]
pub struct VariantInfo {
    /// Variant doc comments.
    pub variant_comments: String,
    /// Pascal-case variant name.
    pub variant_name: String,
    /// Normalized variant cases.
    pub variant_cases: Vec<VariantCaseInfo>,
}

/// Language-normalized variant case metadata.
#[derive(Debug, Clone)]
pub struct VariantCaseInfo {
    /// Case index.
    pub vc_number: u32,
    /// Case doc comments.
    pub vc_comments: String,
    /// Pascal-case case name.
    pub vc_case_name: String,
    /// Snake-case case name.
    pub vc_case_name_snake: String,
    /// Whether the case carries an inner type.
    pub vc_has_inner_type: bool,
    /// Language-specific inner type name.
    pub vc_inner_type_name: String,
    /// Whether the inner type is a C# reference type.
    pub vc_inner_required: bool,
    /// Default-value expression for the inner type.
    pub vc_inner_default_value: String,
    /// Suffix used when deserializing the inner type.
    pub vc_inner_deserialize_suffix: String,
}

impl VariantInfo {
    /// Convert a [`UniVariantDef`] into a [`VariantInfo`] for the target language.
    pub fn from(variant_def: UniVariantDef, lang: LangKind) -> RS<Self> {
        let name = to_pascal_case(&variant_def.variant_name);
        let mut variants = Vec::with_capacity(variant_def.variant_cases.len());
        for (i, v) in variant_def.variant_cases.into_iter().enumerate() {
            let case_ty = v
                .vc_case_type
                .clone()
                .unwrap_or(UniDatType::Scalar(UniScalar::U8));
            let (vc_has_inner_type, vc_inner_type_name) = match &v.vc_case_type {
                Some(ty) => (true, uni_data_type_to_name(ty, &lang)?),
                None => (
                    false,
                    uni_data_type_to_name(&UniDatType::Scalar(UniScalar::U8), &lang)?,
                ),
            };
            let (vc_inner_required, vc_inner_default_value, vc_inner_deserialize_suffix) =
                if lang == LangKind::CSharp {
                    let is_reference = csharp_is_reference_type(&case_ty);
                    (
                        is_reference,
                        csharp_default_value_expr(&case_ty)?,
                        if is_reference {
                            "!".to_string()
                        } else {
                            String::new()
                        },
                    )
                } else {
                    (false, String::new(), String::new())
                };
            let vc = VariantCaseInfo {
                vc_number: i as _,
                vc_comments: v.vc_comments,
                vc_case_name: to_pascal_case(&v.vc_case_name),
                vc_case_name_snake: to_snake_case(&v.vc_case_name),
                vc_has_inner_type,
                vc_inner_type_name,
                vc_inner_required,
                vc_inner_default_value,
                vc_inner_deserialize_suffix,
            };
            variants.push(vc)
        }
        let variant = VariantInfo {
            variant_comments: variant_def.variant_comments,
            variant_name: name,
            variant_cases: variants,
        };
        Ok(variant)
    }
}

#[cfg(test)]
mod tests {
    use super::VariantInfo;
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_dat_type::UniDatType;
    use mudu_binding::universal::uni_def::{UniVariantDef, VariantCase};
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn from_normalizes_variant_for_rust() -> RS<()> {
        let variant_def = UniVariantDef {
            variant_comments: "comment".to_string(),
            variant_name: "mu-dat-value".to_string(),
            variant_cases: vec![
                VariantCase {
                    vc_comments: "i32".to_string(),
                    vc_case_name: "i32".to_string(),
                    vc_case_type: Some(UniDatType::Scalar(UniScalar::I32)),
                },
                VariantCase {
                    vc_comments: "none".to_string(),
                    vc_case_name: "none".to_string(),
                    vc_case_type: None,
                },
            ],
        };
        let info = VariantInfo::from(variant_def, LangKind::Rust)?;
        assert_eq!(info.variant_name, "MuDatValue");
        assert_eq!(info.variant_cases.len(), 2);
        assert_eq!(info.variant_cases[0].vc_case_name, "I32");
        assert_eq!(info.variant_cases[0].vc_inner_type_name, "i32");
        assert!(!info.variant_cases[0].vc_inner_required);
        assert_eq!(info.variant_cases[1].vc_inner_type_name, "u8");
        Ok(())
    }

    #[test]
    fn from_tracks_reference_types_for_csharp() -> RS<()> {
        let variant_def = UniVariantDef {
            variant_comments: String::new(),
            variant_name: "v".to_string(),
            variant_cases: vec![VariantCase {
                vc_comments: String::new(),
                vc_case_name: "text".to_string(),
                vc_case_type: Some(UniDatType::Scalar(UniScalar::String)),
            }],
        };
        let info = VariantInfo::from(variant_def, LangKind::CSharp)?;
        assert!(info.variant_cases[0].vc_inner_required);
        assert_eq!(info.variant_cases[0].vc_inner_default_value, "string.Empty");
        assert_eq!(info.variant_cases[0].vc_inner_deserialize_suffix, "!");
        Ok(())
    }
}
