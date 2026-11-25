use crate::lang_impl::assemblyscript::template_enum_as::TemplateEnumAS;
use crate::lang_impl::assemblyscript::template_file_as::{FileInfo, TemplateFileAS};
use crate::lang_impl::assemblyscript::template_func_as::{TemplateFuncAS, TemplateFuncHeaderAS};
use crate::lang_impl::assemblyscript::template_object_as::TemplateObjectAS;
use crate::lang_impl::assemblyscript::template_struct_as::TemplateStructAS;
use crate::lang_impl::assemblyscript::template_variant_as::TemplateVariantAS;
use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::table::class_def::ClassDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniStructDef, UniVariantDef};
use std::sync::Arc;

/// Create an AssemblyScript renderer.
pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderAS::new())
}

struct RenderAS {}

impl Render for RenderAS {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        // An object module is self-contained (it imports the guest binding
        // facade directly), so it bypasses the sibling-import file wrapper.
        if let [TemplateKind::Object(object)] = template.elements.as_slice() {
            return Self::render_object_as(object);
        }
        // Each WIT `use a:b/c-d/c-d` item maps to an import of the generated
        // module named after the type (PascalCase of the last segment).
        let using_stmts = template
            .using_stmts
            .iter()
            .filter_map(|path| path.last().map(|name| to_pascal_case(name)))
            .collect();
        let blocks = self.render_inner(template.elements)?;
        let template_file = TemplateFileAS {
            file: FileInfo {
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

impl RenderAS {
    fn new() -> Self {
        Self {}
    }

    /// Render one askama template, mapping its failure into a Decode error.
    fn render_template<T: askama::Template>(template: T, what: &str) -> RS<String> {
        template.render().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                format!("render assemblyscript {what} template error"),
                e
            )
        })
    }

    fn render_inner(&self, elements: Vec<TemplateKind>) -> RS<Vec<String>> {
        let mut code_blocks = Vec::with_capacity(elements.len());
        for element in elements {
            let s = match element {
                TemplateKind::Enum((def, cfg)) => Self::render_enum_as(def, cfg)?,
                TemplateKind::Variant((def, cfg)) => Self::render_variant_as(def, cfg)?,
                TemplateKind::Struct((def, cfg)) => Self::render_record_as(def, cfg)?,
                TemplateKind::Object(object) => Self::render_object_as(&object)?,
                TemplateKind::Table(_) => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "table code generation for AssemblyScript is not implemented"
                    ));
                }
                TemplateKind::FuncHeader((funcs, cfg)) => Self::render_func_header_as(funcs, cfg)?,
                TemplateKind::Func((func, cfg)) => Self::render_func_as(func, cfg)?,
            };
            code_blocks.push(s);
        }
        Ok(code_blocks)
    }

    fn render_record_as(def: UniStructDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateStructAS::from(def, cfg)?;
        Self::render_template(template, "record")
    }

    fn render_variant_as(def: UniVariantDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateVariantAS::from(def, cfg)?;
        Self::render_template(template, "variant")
    }

    fn render_enum_as(def: UniEnumDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateEnumAS::from(def, cfg)?;
        Self::render_template(template, "enum")
    }

    fn render_object_as(table_schema: &ClassDef) -> RS<String> {
        let template = TemplateObjectAS::from_table_schema(table_schema)?;
        Self::render_template(template, "object")
    }

    fn render_func_header_as(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncHeaderAS::from(funcs, cfg)?;
        Self::render_template(template, "func header")
    }

    fn render_func_as(func: WitFuncDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncAS::from(func, cfg)?;
        Self::render_template(template, "func")
    }
}
