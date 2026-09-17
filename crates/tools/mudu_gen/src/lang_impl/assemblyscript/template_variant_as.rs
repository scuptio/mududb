use crate::lang_impl::assemblyscript::as_codec::{
    AS_STYLE_RECORD, AsTupleHolder, CollectingTupleNamer, as_codec_default, as_codec_type,
    as_decode_stmts, as_encode_stmts, as_join,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::variant_info::VariantInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniVariantDef;

/// Precomputed explicit-mpack codec statements for one variant case.
pub struct AsCaseCodec {
    /// AssemblyScript inner type (empty when the case has no payload).
    pub inner_ty: String,
    /// Inner default-value expression.
    pub inner_default: String,
    /// Encode statements for the case payload (joined, indented for the
    /// `switch` body); empty when the case has no payload (the `0u8`
    /// placeholder is emitted by the template).
    pub encode_stmts: String,
    /// Decode statements for the case payload (joined, indented for the
    /// `switch` body).
    pub decode_stmts: String,
}

/// Askama template for an AssemblyScript variant.
#[derive(Template)]
#[template(path = "assemblyscript/variant.ts.jinja", escape = "none")]
pub struct TemplateVariantAS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Normalized variant metadata.
    pub variant: VariantInfo,
    /// Per-case codec statements, parallel to `variant.variant_cases`.
    pub case_codecs: Vec<AsCaseCodec>,
    /// Tuple holder classes referenced by tuple-typed case payloads.
    pub tuple_holders: Vec<AsTupleHolder>,
}

impl TemplateVariantAS {
    /// Build the template from a WIT variant definition.
    pub fn from(variant_def: UniVariantDef, cfg: CodegenCfg) -> RS<TemplateVariantAS> {
        let variant = VariantInfo::from(
            variant_def.clone(),
            LangKind::AssemblyScript,
            &cfg.type_kinds,
        )?;
        let mut namer = CollectingTupleNamer::with_kinds(cfg.type_kinds.clone());
        let mut case_codecs = Vec::with_capacity(variant.variant_cases.len());
        for (case, case_def) in variant
            .variant_cases
            .iter()
            .zip(variant_def.variant_cases.iter())
        {
            let base = format!("{}{}", variant.variant_name, case.vc_case_name);
            namer.begin(base.clone());
            let mut enc = Vec::new();
            let mut dec = Vec::new();
            let mut inner_ty = String::new();
            let mut inner_default = String::new();
            if let Some(case_ty) = &case_def.vc_case_type {
                inner_ty = as_codec_type(case_ty, &mut namer)?;
                inner_default = as_codec_default(case_ty, &mut namer)?;
                let access = format!(
                    "(value as {}{}).inner",
                    variant.variant_name, case.vc_case_name
                );
                as_encode_stmts(AS_STYLE_RECORD, case_ty, &access, 16, &mut enc)?;
                namer.begin(base);
                as_decode_stmts(
                    AS_STYLE_RECORD,
                    case_ty,
                    "value.inner",
                    16,
                    &mut dec,
                    &mut namer,
                )?;
            }
            case_codecs.push(AsCaseCodec {
                inner_ty,
                inner_default,
                encode_stmts: as_join(&enc),
                decode_stmts: as_join(&dec),
            });
        }
        let tuple_holders = namer.holders().to_vec();
        Ok(TemplateVariantAS {
            cfg,
            variant,
            case_codecs,
            tuple_holders,
        })
    }
}
