//! Go rendering back-end: message types (records/variants/enums plus the
//! MSSP func codecs) and entity files.

use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniVariantDef};
use std::sync::Arc;

use crate::lang_impl::go::go_codec::{GO_CODEC_IMPORT, go_package_name};
use crate::lang_impl::go::template_entity_go::TemplateEntityGO;
use crate::lang_impl::go::template_enum_go::TemplateEnumGo;
use crate::lang_impl::go::template_file_go::TemplateFileGo;
use crate::lang_impl::go::template_func_go::{TemplateFuncGo, TemplateFuncHeaderGo};
use crate::lang_impl::go::template_record_go::TemplateRecordGo;
use crate::lang_impl::go::template_variant_go::TemplateVariantGo;
use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;

/// Create a Go renderer.
pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderGO::new())
}

struct RenderGO {}

impl Render for RenderGO {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        // An entity file is self-contained (`package main` with its own
        // imports for the mpm-crate Go guest), so it bypasses the message
        // file wrapper.
        if let [TemplateKind::Entity(entity)] = template.elements.as_slice() {
            return Self::render_entity_go(entity);
        }
        let blocks = self.render_inner(template.elements)?;
        // Go rejects unused imports, so the import block is derived from the
        // rendered blocks: `fmt.` marks the checked-conversion error paths,
        // `codec.` marks references to the hand-written runtime package.
        // Generated comments never contain these substrings.
        let joined = blocks.join("\n");
        let mut imports = Vec::new();
        if joined.contains("fmt.") {
            imports.push("fmt".to_string());
        }
        if joined.contains("codec.") {
            imports.push(GO_CODEC_IMPORT.to_string());
        }
        let template_file = TemplateFileGo {
            package: go_package_name(&template.namespace),
            imports,
            blocks,
        };
        let s = template_file
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Internal, "render error", e))?;
        // gofmt wants exactly one trailing newline at end of file.
        Ok(format!("{}\n", s.trim_end()))
    }
}

impl RenderGO {
    fn new() -> Self {
        Self {}
    }

    fn render_inner(&self, elements: Vec<TemplateKind>) -> RS<Vec<String>> {
        let mut code_blocks = Vec::with_capacity(elements.len());
        for element in elements {
            let s = match element {
                TemplateKind::Enum((def, cfg)) => Self::render_enum_go(def, cfg)?,
                TemplateKind::Variant((def, cfg)) => Self::render_variant_go(def, cfg)?,
                TemplateKind::Record((def, cfg)) => Self::render_record_go(def, cfg)?,
                TemplateKind::Entity(entity) => Self::render_entity_go(&entity)?,
                TemplateKind::Table(_) => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "table code generation for Go is not implemented"
                    ));
                }
                TemplateKind::FuncHeader((funcs, cfg)) => Self::render_func_header_go(funcs, cfg)?,
                TemplateKind::Func((func, cfg)) => Self::render_func_go(func, cfg)?,
            };
            // The file wrapper joins blocks with exactly one blank line and
            // gofmt collapses longer runs, so trailing whitespace goes here.
            code_blocks.push(s.trim_end().to_string());
        }
        Ok(code_blocks)
    }

    fn render_entity_go(table_schema: &TableDef) -> RS<String> {
        let template = TemplateEntityGO::from_table_schema(table_schema)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render go entity template error", e))?;
        Ok(s)
    }

    fn render_record_go(def: UniRecordDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateRecordGo::from(def, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render go record template error", e))?;
        Ok(s)
    }

    fn render_variant_go(def: UniVariantDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateVariantGo::from(def, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render go variant template error", e))?;
        Ok(s)
    }

    fn render_enum_go(def: UniEnumDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateEnumGo::from(def, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render go enum template error", e))?;
        Ok(s)
    }

    fn render_func_header_go(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncHeaderGo::from(funcs, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render go func header template error", e)
        })?;
        Ok(s)
    }

    fn render_func_go(func: WitFuncDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncGo::from(func, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render go func template error", e))?;
        Ok(s)
    }
}
