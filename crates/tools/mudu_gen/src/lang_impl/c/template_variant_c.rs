//! Askama template data for a C variant (tag enum plus tagged-union struct
//! with `static inline` MessagePack codecs).
//!
//! The wire shape is a `[tag, payload]` 2-array; payload-less cases carry a
//! `0u8` placeholder (NOT nil), matching the Rust host's variant serde.
//!
//! Payload representation: scalars, `mp_str`/`mp_bin` slices, enums and
//! generated composite holders ride BY VALUE in the union; payloads of named
//! record/variant types are POINTERS (arena-owned on decode, borrowed on
//! encode) — this keeps every cross-file type reference pointer-sized, which
//! is what makes the per-file self-contained headers acyclic. A payload-less
//! case has no union member; the active case is the `<type>_kind` tag.

use crate::lang_impl::c::c_codec::{
    C_STYLE_RECORD, CollectingCNamer, c_decode_stmts, c_encode_stmts, c_ident, c_join, c_type,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::variant_info::VariantInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case, to_snake_case_upper};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::UniVariantDef;

/// One case of the generated variant kind enum.
pub struct CKindCase {
    /// Case doc comments (`//` lines; empty when undocumented).
    pub comments: String,
    /// Enumerator constant (`UNI_SCALAR_VALUE_BOOL`).
    pub const_name: String,
    /// 0-based case number (the wire tag).
    pub number: u32,
}

/// Whether a variant payload of a named type is a pointer union member.
/// Record/variant payloads (and unregistered names, which follow the record
/// assumption) are pointers; enum payloads stay by value.
fn payload_by_pointer(ty: &UniDataType, cfg: &CodegenCfg) -> bool {
    match ty {
        UniDataType::Identifier(name) => !matches!(
            cfg.type_kinds
                .get(&to_pascal_case(name))
                .map(|k| k.as_str()),
            Some("enum")
        ),
        _ => false,
    }
}

/// The C representation of a case payload: named record/variant payloads
/// become `box<T>` (pointer, arena-owned); everything else stays as
/// declared.
fn payload_repr(ty: &UniDataType, cfg: &CodegenCfg) -> UniDataType {
    if payload_by_pointer(ty, cfg) {
        UniDataType::Box(Box::new(ty.clone()))
    } else {
        ty.clone()
    }
}

/// Askama template for a C variant struct (kind enum, holders, union).
#[derive(Template)]
#[template(path = "c/variant.h.jinja", escape = "none")]
pub struct TemplateVariantC {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Variant doc comments (`//` lines; empty when undocumented).
    pub comments: String,
    /// Snake-case variant name (`uni_scalar_value`).
    pub name: String,
    /// Snake-case kind enum name (`uni_scalar_value_kind`).
    pub kind_name: String,
    /// Kind enum cases in declaration order.
    pub kind_cases: Vec<CKindCase>,
    /// Precomputed composite holder typedefs referenced by the payloads.
    pub holder_decls: Vec<String>,
    /// Union member declarations (one per payload-carrying case), rendered.
    pub union_members: Vec<String>,
    /// Whether any case carries a payload (otherwise no union is emitted).
    pub has_union: bool,
    /// Encode function body (indented for the function).
    pub encode_body: String,
    /// Decode function body (indented for the function).
    pub decode_body: String,
}

/// Askama template for a C variant's codec functions (encode/decode plus
/// the arena-allocating `<type>_decode_new`), borrowing the struct data.
#[derive(Template)]
#[template(path = "c/variant_codec.h.jinja", escape = "none")]
pub struct TemplateVariantCodecC<'a> {
    /// The struct template data built by [`TemplateVariantC::from`].
    pub data: &'a TemplateVariantC,
}

impl TemplateVariantC {
    /// Build the template data from a WIT variant definition.
    pub fn from(variant_def: UniVariantDef, cfg: CodegenCfg) -> RS<Self> {
        let variant = VariantInfo::from(variant_def.clone(), LangKind::C, &cfg.type_kinds)?;
        let name = to_snake_case(&variant.variant_name);
        let const_prefix = to_snake_case_upper(&variant.variant_name);
        let mut namer = CollectingCNamer::new();
        let mut kind_cases = Vec::with_capacity(variant.variant_cases.len());
        let mut union_members = Vec::new();
        let mut enc_lines: Vec<String> = Vec::new();
        let mut dec_lines: Vec<String> = Vec::new();
        let mut has_union = false;
        for (case, case_def) in variant
            .variant_cases
            .iter()
            .zip(variant_def.variant_cases.iter())
        {
            let case_name = c_ident(&to_snake_case(&case_def.vc_case_name));
            let const_name = format!(
                "{}_{}",
                const_prefix,
                to_snake_case_upper(&case_def.vc_case_name)
            );
            kind_cases.push(CKindCase {
                comments: case.vc_comments.clone(),
                const_name: const_name.clone(),
                number: case.vc_number,
            });
            let base = format!("{}_{}", name, case_name);
            enc_lines.push(format!("    case {}:", const_name));
            dec_lines.push(format!("        case {}u:", case.vc_number));
            if let Some(case_ty) = &case_def.vc_case_type {
                has_union = true;
                let repr = payload_repr(case_ty, &cfg);
                namer.begin(base.clone());
                let ty = c_type(&repr, &mut namer)?;
                union_members.push(format!("        {} {};", ty, case_name));
                c_encode_stmts(
                    C_STYLE_RECORD,
                    &repr,
                    &format!("value->as.{}", case_name),
                    8,
                    0,
                    &mut enc_lines,
                )?;
                dec_lines.push(format!("            value->kind = {};", const_name));
                namer.begin(base);
                c_decode_stmts(
                    C_STYLE_RECORD,
                    &repr,
                    &format!("value->as.{}", case_name),
                    12,
                    0,
                    &mut dec_lines,
                    &mut namer,
                )?;
            } else {
                enc_lines.push(
                    "        /* payload-less case: the `0u8` placeholder (NOT nil), matching"
                        .to_string(),
                );
                enc_lines.push("         * the Rust host's variant serde. */".to_string());
                enc_lines.push("        mpw_u64(w, 0u);".to_string());
                dec_lines.push(format!("            value->kind = {};", const_name));
                dec_lines.push("            /* consume the `0u8` placeholder */".to_string());
                dec_lines.push("            (void)mpr_u64(r);".to_string());
            }
            enc_lines.push("        break;".to_string());
            dec_lines.push("            break;".to_string());
        }
        let mut encode_body = vec![
            "    mpw_array_header(w, 2u);".to_string(),
            "    mpw_u64(w, (uint64_t)value->kind);".to_string(),
            "    switch (value->kind) {".to_string(),
        ];
        encode_body.extend(enc_lines);
        encode_body.push("    default:".to_string());
        encode_body.push(
            "        /* unknown kind: flag the writer instead of emitting a corrupt value */"
                .to_string(),
        );
        encode_body.push("        mpw_fail(w);".to_string());
        encode_body.push("        break;".to_string());
        encode_body.push("    }".to_string());

        let mut decode_body = vec![
            "    (void)a;".to_string(),
            "    memset(value, 0, sizeof(*value));".to_string(),
            "    if (mpr_array_header(r) != 2u) {".to_string(),
            "        mpr_fail(r);".to_string(),
            "        return -1;".to_string(),
            "    }".to_string(),
            "    {".to_string(),
            "        uint64_t tag = mpr_u64(r);".to_string(),
            "        switch (tag) {".to_string(),
        ];
        decode_body.extend(dec_lines);
        decode_body.push("        default:".to_string());
        decode_body.push("            mpr_fail(r);".to_string());
        decode_body.push("            return -1;".to_string());
        decode_body.push("        }".to_string());
        decode_body.push("    }".to_string());
        decode_body.push("    return mpr_ok(r) ? 0 : -1;".to_string());

        let holder_decls = namer.holders().iter().map(|h| h.decl()).collect();
        Ok(Self {
            cfg,
            comments: variant.variant_comments,
            kind_name: format!("{}_kind", name),
            name,
            kind_cases,
            holder_decls,
            union_members,
            has_union,
            encode_body: c_join(&encode_body),
            decode_body: c_join(&decode_body),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{TemplateVariantC, TemplateVariantCodecC};
    use crate::src_gen::codegen_cfg::CodegenCfg;
    use askama::Template;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::{UniVariantDef, VariantCase};
    use mudu_binding::universal::uni_scalar::UniScalar;

    fn render(def: UniVariantDef) -> RS<(String, String)> {
        let template = TemplateVariantC::from(def, CodegenCfg::new())?;
        let struct_part = template.render().unwrap();
        let codec_part = (TemplateVariantCodecC { data: &template })
            .render()
            .unwrap();
        Ok((struct_part, codec_part))
    }

    fn scalar_value_def() -> UniVariantDef {
        UniVariantDef {
            variant_comments: String::new(),
            variant_name: "uni-scalar-value".to_string(),
            variant_cases: vec![
                VariantCase {
                    vc_comments: String::new(),
                    vc_case_name: "bool".to_string(),
                    vc_case_type: Some(UniDataType::Scalar(UniScalar::Bool)),
                },
                VariantCase {
                    vc_comments: String::new(),
                    vc_case_name: "blob".to_string(),
                    vc_case_type: Some(UniDataType::Binary),
                },
                VariantCase {
                    vc_comments: String::new(),
                    vc_case_name: "null".to_string(),
                    vc_case_type: None,
                },
            ],
        }
    }

    #[test]
    fn renders_kind_enum_union_and_placeholder() -> RS<()> {
        let (struct_part, codec_part) = render(scalar_value_def())?;
        assert!(struct_part.contains("} uni_scalar_value_kind;"));
        assert!(struct_part.contains("UNI_SCALAR_VALUE_BOOL = 0u,"));
        assert!(struct_part.contains("UNI_SCALAR_VALUE_NULL = 2u,"));
        assert!(struct_part.contains("bool bool_;"));
        assert!(struct_part.contains("mp_bin blob;"));
        assert!(struct_part.contains("} as;"));
        // payload-less case writes the 0u8 placeholder on encode ...
        assert!(codec_part.contains("mpw_u64(w, 0u);"));
        // ... and consumes it on decode
        assert!(codec_part.contains("(void)mpr_u64(r);"));
        // record-context `list<u8>`: a MessagePack ARRAY of u8, not bin
        assert!(codec_part.contains("mpw_array_header(w, (value->as.blob).len);"));
        assert!(!codec_part.contains("mpw_bin"));
        Ok(())
    }

    #[test]
    fn named_record_payloads_are_pointer_members() -> RS<()> {
        let mut cfg = CodegenCfg::new();
        cfg.type_kinds
            .insert("UniDataType".to_string(), "variant".to_string());
        let def = UniVariantDef {
            variant_comments: String::new(),
            variant_name: "uni-data-value".to_string(),
            variant_cases: vec![VariantCase {
                vc_comments: String::new(),
                vc_case_name: "scalar".to_string(),
                vc_case_type: Some(UniDataType::Identifier("uni-data-type".to_string())),
            }],
        };
        let template = TemplateVariantC::from(def, cfg)?;
        let struct_part = template.render().unwrap();
        let codec_part = (TemplateVariantCodecC { data: &template })
            .render()
            .unwrap();
        // pointer union member; encode NULL-checks; decode goes through
        // `<T>_decode_new` (no sizeof of the pointee in this header)
        assert!(struct_part.contains("uni_data_type * scalar;"));
        assert!(codec_part.contains("if ((value->as.scalar) == 0) {"));
        assert!(codec_part.contains("uni_data_type_encode(w, &((*value->as.scalar)));"));
        assert!(codec_part.contains("uni_data_type *p0 = uni_data_type_decode_new(r, a);"));
        assert!(codec_part.contains("value->as.scalar = p0;"));
        // the arena-allocating decoder of this type itself
        assert!(codec_part.contains(
            "MP_INLINE uni_data_value *uni_data_value_decode_new(mp_reader *r, mp_arena *a) {"
        ));
        assert!(codec_part.contains("(uni_data_value *)mpa_alloc(a, sizeof(uni_data_value))"));
        Ok(())
    }

    #[test]
    fn enum_payloads_stay_by_value() -> RS<()> {
        let mut cfg = CodegenCfg::new();
        cfg.type_kinds
            .insert("UniScalar".to_string(), "enum".to_string());
        let def = UniVariantDef {
            variant_comments: String::new(),
            variant_name: "uni-data-type".to_string(),
            variant_cases: vec![VariantCase {
                vc_comments: String::new(),
                vc_case_name: "scalar".to_string(),
                vc_case_type: Some(UniDataType::Identifier("uni-scalar".to_string())),
            }],
        };
        let template = TemplateVariantC::from(def, cfg)?;
        let struct_part = template.render().unwrap();
        assert!(struct_part.contains("uni_scalar scalar;"));
        Ok(())
    }
}
