use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::struct_info::StructInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniStructDef;

/// Askama template for a C# record.
#[derive(Template)]
#[template(path = "csharp/struct.cs.jinja", escape = "none")]
pub struct TemplateStructCS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Normalized record metadata.
    pub record: StructInfo,
    /// Whether any field is a WIT `option<T>`; the encoder only emits the
    /// omit-when-absent guarded form then (option-less records keep the
    /// historical fixed-header shape).
    pub has_option_field: bool,
}

impl TemplateStructCS {
    /// Build the template from a WIT record definition.
    pub fn from(record_def: UniStructDef, cfg: CodegenCfg) -> RS<Self> {
        let record = StructInfo::from(record_def, LangKind::CSharp, &cfg.type_kinds)?;
        let has_option_field = record.struct_fields.iter().any(|f| f.rf_is_option);
        Ok(Self {
            record,
            has_option_field,
            cfg,
        })
    }
}
