use askama::Template;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::record::record_def::RecordDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniVariantDef};
use std::sync::Arc;

use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::lang_impl::rust::template_entity_rs::TemplateEntityRS;
use crate::lang_impl::rust::template_enum_rs::TemplateEnumRS;
use crate::lang_impl::rust::template_record_rs::TemplateRecordRS;
use crate::lang_impl::rust::template_variant_rs::TemplateVariantRS;
use crate::src_gen::codegen_cfg::CodegenCfg;

pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderRS::new())
}

struct RenderRS {

}

impl Render for RenderRS {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        let mut source_code = String::new();
        for using_stmt in template.using_stmts {
            let s = Self::render_use(using_stmt)?;
            source_code.push_str(&s);
            source_code.push('\n');
        }
        let blocks = self.render_inner(template.elements)?;
        for block in blocks {
            source_code.push_str(&block);
            source_code.push('\n');
        }
        Ok(source_code)
    }
}

impl RenderRS {
    fn new() -> Self {
        Self {}
    }

    fn render_inner(&self, elements: Vec<TemplateKind>) -> RS<Vec<String>> {
        let mut code_block = Vec::with_capacity(elements.len());
        for element in  elements {
            match element {
                TemplateKind::Enum((def, cfg)) => {
                    let s = Self::render_enum_rs(def, cfg)?;
                    code_block.push(s);
                }
                TemplateKind::Variant((def, cfg)) => {
                    let s = Self::render_variant_rs(def, cfg)?;
                    code_block.push(s);
                }
                TemplateKind::Record((def, cfg)) => {
                    let s = Self::render_record_rs(def, cfg)?;
                    code_block.push(s);
                }
                TemplateKind::Entity(entity) => {
                    let s = Self::render_entity_rs(entity)?;
                    code_block.push(s);
                }
            }
        }
        Ok(code_block)
    }

    fn render_use(path: Vec<String>) -> RS<String> {
        let mut s = "use ".to_string();
        let n = path.len();
        for (i, mut name) in path.into_iter().enumerate() {
            if i != n - 1 {
                name = to_snake_case(&name);
            } else {
                name = to_pascal_case(&name);
            }
            if i != 0 {
                s.push_str("::")
            }
            s.push_str(&name);
        }
        s.push_str(";\n");
        Ok(s)
    }

    fn render_record_rs(def:UniRecordDef, cfg:CodegenCfg) -> RS<String> {
        let template = TemplateRecordRS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            m_error!(EC::DecodeErr, "render rust record template error", e) })?;
        Ok(s)
    }

    fn render_enum_rs(def:UniEnumDef, cfg:CodegenCfg) -> RS<String> {
        let template = TemplateEnumRS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            m_error!(EC::DecodeErr, "render rust enum template error", e)
        })?;
        Ok(s)
    }

    fn render_variant_rs(def:UniVariantDef, cfg:CodegenCfg) -> RS<String> {
        let template = TemplateVariantRS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            m_error!(EC::DecodeErr, "render rust variant template error", e)
        })?;
        Ok(s)
    }

    fn render_entity_rs(table_schema: RecordDef) -> RS<String> {
        let template = TemplateEntityRS::from_table_schema(&table_schema)?;
        let s = template.render().map_err(|e| {
            m_error!(EC::DecodeErr, "render rust entity template error", e)
        })?;
        Ok(s)
    }
}