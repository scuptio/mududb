use crate::lang_impl::csharp::codec_expr_cs;
use crate::lang_impl::lang::lang_data_type::{
    assemblyscript_default_value_expr, csharp_default_value_expr, csharp_is_reference_type,
    uni_data_type_to_name,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::python::py_codec::{
    PY_RECORD_BIN, py_field_default, py_from_value, py_to_value,
};
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::{UniVariantDef, VariantCase};
use mudu_binding::universal::uni_scalar::UniScalar;
use std::collections::HashMap;

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
    #[allow(dead_code)] // superseded by `vc_inner_deserialize` in the C# template
    pub vc_inner_deserialize_suffix: String,
    /// C# resolver-free serialize statements writing the inner value to
    /// `writer` (empty for other languages and unit cases).
    pub vc_inner_serialize: String,
    /// C# resolver-free deserialize statements reading the inner value from
    /// `reader` into the `{case}Inner` local (empty for other languages and
    /// unit cases).
    pub vc_inner_deserialize: String,
    /// Wire expression converting the inner value to a wire value (Rust:
    /// the `inner` binding (`&T`) to `Value`; Python: `x.inner` to a native
    /// value; empty for other languages and unit cases).
    pub vc_to_value: String,
    /// Wire expression converting the variant payload slot into the typed
    /// inner value (Rust: `&items[1]` to `Result<InnerType, WireError>`;
    /// Python: `v[1]` to the inner value; empty for other languages and unit
    /// cases).
    pub vc_from_value: String,
}

impl VariantInfo {
    /// Convert a [`UniVariantDef`] into a [`VariantInfo`] for the target language.
    pub fn from(
        variant_def: UniVariantDef,
        lang: LangKind,
        type_kinds: &HashMap<String, String>,
    ) -> RS<Self> {
        let name = to_pascal_case(&variant_def.variant_name);
        let mut variants = Vec::with_capacity(variant_def.variant_cases.len());
        for (i, v) in variant_def.variant_cases.into_iter().enumerate() {
            variants.push(case_info_at(i, v, &name, &lang, type_kinds)?);
        }
        let variant = VariantInfo {
            variant_comments: variant_def.variant_comments,
            variant_name: name,
            variant_cases: variants,
        };
        Ok(variant)
    }
}

/// Build the language-normalized [`VariantCaseInfo`] of one variant case.
fn case_info_at(
    i: usize,
    v: VariantCase,
    name: &str,
    lang: &LangKind,
    type_kinds: &HashMap<String, String>,
) -> RS<VariantCaseInfo> {
    let case_ty = v
        .vc_case_type
        .clone()
        .unwrap_or(UniDataType::Scalar(UniScalar::U8));
    let (vc_has_inner_type, vc_inner_type_name) = match &v.vc_case_type {
        Some(ty) => (true, uni_data_type_to_name(ty, lang)?),
        None => (
            false,
            uni_data_type_to_name(&UniDataType::Scalar(UniScalar::U8), lang)?,
        ),
    };
    let (vc_inner_required, vc_inner_default_value, vc_inner_deserialize_suffix) =
        case_defaults(&case_ty, lang, type_kinds)?;
    // Resolver-free C# read/write statements for the case inner
    // value; other languages leave these empty.
    let (vc_inner_serialize, vc_inner_deserialize) =
        if *lang == LangKind::CSharp && vc_has_inner_type {
            case_ser_de_cs(
                &case_ty,
                name,
                &v.vc_case_name,
                &vc_inner_type_name,
                vc_inner_required,
                type_kinds,
            )?
        } else {
            (String::new(), String::new())
        };
    let (vc_to_value, vc_from_value) = case_wire_exprs(&case_ty, vc_has_inner_type, lang)?;
    Ok(VariantCaseInfo {
        vc_number: i as _,
        vc_comments: v.vc_comments,
        vc_case_name: to_pascal_case(&v.vc_case_name),
        vc_case_name_snake: to_snake_case(&v.vc_case_name),
        vc_has_inner_type,
        vc_inner_type_name,
        vc_inner_required,
        vc_inner_default_value,
        vc_inner_deserialize_suffix,
        vc_inner_serialize,
        vc_inner_deserialize,
        vc_to_value,
        vc_from_value,
    })
}

/// Per-language `(inner-required, inner default-value expression, inner
/// deserialize suffix)` of a variant case payload.
fn case_defaults(
    case_ty: &UniDataType,
    lang: &LangKind,
    type_kinds: &HashMap<String, String>,
) -> RS<(bool, String, String)> {
    Ok(match lang {
        LangKind::CSharp => {
            let is_reference = csharp_is_reference_type(case_ty);
            (
                is_reference,
                csharp_default_value_expr(case_ty, type_kinds)?,
                if is_reference {
                    "!".to_string()
                } else {
                    String::new()
                },
            )
        }
        LangKind::AssemblyScript => (
            false,
            assemblyscript_default_value_expr(case_ty)?,
            String::new(),
        ),
        LangKind::Rust => (false, String::new(), String::new()),
        LangKind::Python => (false, py_field_default(case_ty, type_kinds)?, String::new()),
        LangKind::C | LangKind::Go => (false, String::new(), String::new()),
    })
}

/// C# resolver-free serialize/deserialize statements of a payload-carrying
/// case's inner value.
fn case_ser_de_cs(
    case_ty: &UniDataType,
    name: &str,
    case_name: &str,
    inner_type_name: &str,
    inner_required: bool,
    type_kinds: &HashMap<String, String>,
) -> RS<(String, String)> {
    let mut tmp = 0;
    let inner_expr = format!("(({}{})value).Inner", name, to_pascal_case(case_name));
    let local = format!("{}Inner", to_snake_case(case_name));
    Ok((
        codec_expr_cs::serialize_stmts(
            case_ty,
            &inner_expr,
            "                ",
            &mut tmp,
            type_kinds,
        )?,
        codec_expr_cs::deserialize_local(
            case_ty,
            inner_type_name,
            &local,
            inner_required,
            "                ",
            &mut tmp,
        )?,
    ))
}

/// Wire-runtime conversion expressions for the case inner value;
/// other languages and unit cases leave these empty. Rust converts
/// the `inner` binding (`&T`) through the `mp_wire` `Value` model;
/// Python converts the typed `x.inner` place (encode) and the
/// payload slot `v[1]` (decode) through the native value model.
fn case_wire_exprs(
    case_ty: &UniDataType,
    has_inner_type: bool,
    lang: &LangKind,
) -> RS<(String, String)> {
    if *lang == LangKind::Rust && has_inner_type {
        Ok((
            crate::lang_impl::rust::value_expr_rs::rust_to_value(case_ty, "inner", false)?,
            crate::lang_impl::rust::value_expr_rs::rust_from_value(case_ty, "&items[1]")?,
        ))
    } else if *lang == LangKind::Python && has_inner_type {
        Ok((
            py_to_value(case_ty, "x.inner", PY_RECORD_BIN)?,
            py_from_value(case_ty, "v[1]")?,
        ))
    } else {
        Ok((String::new(), String::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::VariantInfo;
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::{UniVariantDef, VariantCase};
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn from_normalizes_variant_for_rust() -> RS<()> {
        let variant_def = UniVariantDef {
            variant_comments: "comment".to_string(),
            variant_name: "mu-data-value".to_string(),
            variant_cases: vec![
                VariantCase {
                    vc_comments: "i32".to_string(),
                    vc_case_name: "i32".to_string(),
                    vc_case_type: Some(UniDataType::Scalar(UniScalar::I32)),
                },
                VariantCase {
                    vc_comments: "none".to_string(),
                    vc_case_name: "none".to_string(),
                    vc_case_type: None,
                },
            ],
        };
        let info = VariantInfo::from(variant_def, LangKind::Rust, &Default::default())?;
        assert_eq!(info.variant_name, "MuDataValue");
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
                vc_case_type: Some(UniDataType::Scalar(UniScalar::String)),
            }],
        };
        let info = VariantInfo::from(variant_def, LangKind::CSharp, &Default::default())?;
        assert!(info.variant_cases[0].vc_inner_required);
        assert_eq!(info.variant_cases[0].vc_inner_default_value, "string.Empty");
        assert_eq!(info.variant_cases[0].vc_inner_deserialize_suffix, "!");
        Ok(())
    }
}
