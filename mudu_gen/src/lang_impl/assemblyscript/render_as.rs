use crate::lang_impl::assemblyscript::template_enum_as::TemplateEnumAS;
use crate::lang_impl::assemblyscript::template_file_as::{FileInfo, TemplateFileAS};
use crate::lang_impl::assemblyscript::template_record_as::TemplateRecordAS;
use crate::lang_impl::assemblyscript::template_variant_as::TemplateVariantAS;
use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniVariantDef};
use std::sync::Arc;

/// Create an AssemblyScript renderer.
pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderAS::new())
}

struct RenderAS {}

impl Render for RenderAS {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        let blocks = self.render_inner(template.elements)?;
        let template_file = TemplateFileAS {
            file: FileInfo {
                using_stmts: vec![],
                blocks,
            },
        };
        let s = template_file
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Internal, "render error", e))?;
        Ok(s)
    }
}

impl RenderAS {
    fn new() -> Self {
        Self {}
    }

    fn render_inner(&self, elements: Vec<TemplateKind>) -> RS<Vec<String>> {
        let mut code_blocks = Vec::with_capacity(elements.len());
        for element in elements {
            let s = match element {
                TemplateKind::Enum((def, cfg)) => Self::render_enum_as(def, cfg)?,
                TemplateKind::Variant((def, cfg)) => Self::render_variant_as(def, cfg)?,
                TemplateKind::Record((def, cfg)) => Self::render_record_as(def, cfg)?,
                TemplateKind::Entity(_) => String::new(),
                TemplateKind::Table(_) => String::new(),
            };
            code_blocks.push(s);
        }
        Ok(code_blocks)
    }

    fn render_record_as(def: UniRecordDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateRecordAS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "render assemblyscript record template error",
                e
            )
        })?;
        Ok(s)
    }

    fn render_variant_as(def: UniVariantDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateVariantAS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "render assemblyscript variant template error",
                e
            )
        })?;
        Ok(s)
    }

    fn render_enum_as(def: UniEnumDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateEnumAS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "render assemblyscript enum template error",
                e
            )
        })?;
        Ok(s)
    }
}
