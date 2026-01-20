use crate::lang_impl::csharp::template_enum_cs::TemplateEnumCS;
use crate::lang_impl::csharp::template_file_cs::{FileInfo, TemplateFileCS};
use crate::lang_impl::csharp::template_record_cs::TemplateRecordCS;
use crate::lang_impl::csharp::template_variant_cs::TemplateVariantCS;
use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_binding::record::record_def::RecordDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniVariantDef};
use std::sync::Arc;

pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderCS::new())
}

struct RenderCS {

}

impl Render for RenderCS {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        let namespace = template.namespace.clone();
        let blocks = self.render_inner(template.elements)?;
        let template_file = TemplateFileCS {
            file: FileInfo {
                namespace: namespace.clone(),
                using_stmts: vec![],
                blocks,
            },
        };
        let s = template_file.render().map_err(|e|{
            m_error!(EC::InternalErr, "render error", e)
        })?;
        Ok(s)
    }
}

impl RenderCS {
    fn new() -> Self {
        Self {}
    }

    fn render_inner(&self, elements: Vec<TemplateKind>) -> RS<Vec<String>> {
        let mut code_blocks = Vec::with_capacity(elements.len());
        for element in  elements {
            let s = match element {
                TemplateKind::Enum((def, cfg)) => {
                    Self::render_enum_cs(def, cfg)?
                }
                TemplateKind::Variant((def, cfg)) => {
                    Self::render_variant_cs(def, cfg)?
                }
                TemplateKind::Record((def, cfg)) => {
                    Self::render_record_cs(def, cfg)?
                }
                TemplateKind::Entity(entity) => {
                    Self::render_entity_cs(entity)?
                }
            };
            code_blocks.push(s);
        }
        Ok(code_blocks)
    }


    fn render_record_cs(def:UniRecordDef, cfg:CodegenCfg) -> RS<String> {
        let template = TemplateRecordCS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            m_error!(EC::DecodeErr, "render csharp record template error", e) })?;
        Ok(s)
    }

    fn render_enum_cs(def:UniEnumDef, cfg:CodegenCfg) -> RS<String> {
        let template = TemplateEnumCS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            m_error!(EC::DecodeErr, "render csharp enum template error", e) })?;
        Ok(s)
    }

    fn render_variant_cs(def:UniVariantDef, cfg:CodegenCfg) -> RS<String> {
        let template = TemplateVariantCS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            m_error!(EC::DecodeErr, "render csharp variant template error", e) })?;
        Ok(s)
    }
    
    fn render_entity_cs(_: RecordDef) -> RS<String> {
        Ok(String::new())
    }
}