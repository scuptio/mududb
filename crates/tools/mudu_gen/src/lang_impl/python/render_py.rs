//! Python rendering back-end.

use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::lang_impl::python::template_entity_py::TemplateEntityPy;
use crate::lang_impl::python::template_enum_py::TemplateEnumPy;
use crate::lang_impl::python::template_file_py::{FileInfoPy, TemplateFilePy};
use crate::lang_impl::python::template_func_py::{TemplateFuncHeaderPy, TemplateFuncPy};
use crate::lang_impl::python::template_record_py::TemplateRecordPy;
use crate::lang_impl::python::template_variant_py::TemplateVariantPy;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniVariantDef};
use std::sync::Arc;

/// Create a Python renderer.
pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderPy::new())
}

struct RenderPy {}

impl Render for RenderPy {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        // An entity module is self-contained (it imports the `mududb` guest
        // facade directly), so it bypasses the mpack-prelude file wrapper.
        if let [TemplateKind::Entity(entity)] = template.elements.as_slice() {
            return Self::render_entity_py(entity);
        }
        // Each WIT `use a:b/c-d/c-d` item maps to an import of the generated
        // sibling module named after the type (snake_case of the last
        // segment): the type itself plus its conversion functions.
        let using_stmts = template
            .using_stmts
            .iter()
            .filter_map(|path| {
                path.last().map(|name| {
                    let snake = to_snake_case(name);
                    format!(
                        "from .{} import {}, {}_from_value, {}_to_value",
                        snake,
                        to_pascal_case(name),
                        snake,
                        snake
                    )
                })
            })
            .collect();
        let blocks = self.render_inner(template.elements)?;
        let template_file = TemplateFilePy {
            file: FileInfoPy {
                using_stmts,
                blocks,
            },
        };
        let s = template_file
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Internal, "render error", e))?;
        Ok(s)
    }
}

impl RenderPy {
    fn new() -> Self {
        Self {}
    }

    fn render_inner(&self, elements: Vec<TemplateKind>) -> RS<Vec<String>> {
        let mut code_blocks = Vec::with_capacity(elements.len());
        for element in elements {
            let s = match element {
                TemplateKind::Enum((def, cfg)) => Self::render_enum_py(def, cfg)?,
                TemplateKind::Variant((def, cfg)) => Self::render_variant_py(def, cfg)?,
                TemplateKind::Record((def, cfg)) => Self::render_record_py(def, cfg)?,
                TemplateKind::Entity(entity) => Self::render_entity_py(&entity)?,
                TemplateKind::Table(_) => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "table code generation for Python is not implemented"
                    ));
                }
                TemplateKind::FuncHeader((funcs, cfg)) => Self::render_func_header_py(funcs, cfg)?,
                TemplateKind::Func((func, cfg)) => Self::render_func_py(func, cfg)?,
            };
            code_blocks.push(s);
        }
        Ok(code_blocks)
    }

    fn render_record_py(def: UniRecordDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateRecordPy::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render python record template error", e)
        })?;
        Ok(s)
    }

    fn render_variant_py(def: UniVariantDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateVariantPy::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render python variant template error", e)
        })?;
        Ok(s)
    }

    fn render_enum_py(def: UniEnumDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateEnumPy::from(def, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render python enum template error", e))?;
        Ok(s)
    }

    fn render_entity_py(table_schema: &TableDef) -> RS<String> {
        let template = TemplateEntityPy::from_table_schema(table_schema)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render python entity template error", e)
        })?;
        Ok(s)
    }

    fn render_func_header_py(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncHeaderPy::from(funcs, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "render python func header template error",
                e
            )
        })?;
        Ok(s)
    }

    fn render_func_py(func: WitFuncDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncPy::from(func, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render python func template error", e))?;
        Ok(s)
    }
}
