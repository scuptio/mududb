use crate::lang_impl::csharp::template_entity_cs::TemplateEntityCS;
use crate::lang_impl::csharp::template_enum_cs::TemplateEnumCS;
use crate::lang_impl::csharp::template_file_cs::{FileInfo, TemplateFileCS};
use crate::lang_impl::csharp::template_func_cs::{TemplateFuncCS, TemplateFuncHeaderCS};
use crate::lang_impl::csharp::template_record_cs::TemplateRecordCS;
use crate::lang_impl::csharp::template_table_cs::TemplateTableCS;
use crate::lang_impl::csharp::template_variant_cs::TemplateVariantCS;
use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::canonical;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniTableDef, UniVariantDef};
use std::sync::Arc;

/// Create a C# renderer.
pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderCS::new())
}

struct RenderCS {}

impl Render for RenderCS {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        // An entity file is self-contained in the global namespace (mgen
        // does not know the project namespace), so it bypasses the
        // namespace/MessagePack file wrapper.
        if let [TemplateKind::Entity(entity)] = template.elements.as_slice() {
            return Self::render_entity_cs(entity.clone());
        }
        let has_func_codec = template
            .elements
            .iter()
            .any(|e| matches!(e, TemplateKind::FuncHeader(_) | TemplateKind::Func(_)));
        let (namespace, using_stmts) = canonical_cs_namespace(&template.namespace, has_func_codec);
        let blocks = self.render_inner(template.elements)?;
        let template_file = TemplateFileCS {
            file: FileInfo {
                namespace: namespace.clone(),
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

/// Map the WIT interface namespace onto the canonical C# namespace.
///
/// The `universal` interface (the mududb binding surface) renders as
/// lowercase `mududb.types` for the value types, or `mududb.codec` for the
/// MSSP frame codec (detected by the presence of func-codec elements), with
/// the codec file importing `mududb.types`. Any other namespace keeps the
/// historical PascalCase behavior.
fn canonical_cs_namespace(namespace: &str, has_func_codec: bool) -> (String, Vec<String>) {
    if namespace == canonical::UNIVERSAL_INTERFACE {
        let types_ns = format!("{}.{}", canonical::CANONICAL_ROOT, canonical::SEGMENT_TYPES);
        if has_func_codec {
            (
                format!("{}.{}", canonical::CANONICAL_ROOT, canonical::SEGMENT_CODEC),
                vec![types_ns],
            )
        } else {
            (types_ns, vec![])
        }
    } else {
        (to_pascal_case(namespace), vec![])
    }
}

impl RenderCS {
    fn new() -> Self {
        Self {}
    }

    fn render_inner(&self, elements: Vec<TemplateKind>) -> RS<Vec<String>> {
        let mut code_blocks = Vec::with_capacity(elements.len());
        for element in elements {
            let s = match element {
                TemplateKind::Enum((def, cfg)) => Self::render_enum_cs(def, cfg)?,
                TemplateKind::Variant((def, cfg)) => Self::render_variant_cs(def, cfg)?,
                TemplateKind::Record((def, cfg)) => Self::render_record_cs(def, cfg)?,
                TemplateKind::Entity(entity) => Self::render_entity_cs(entity)?,
                TemplateKind::Table((def, cfg)) => Self::render_table_cs(def, cfg)?,
                TemplateKind::FuncHeader((funcs, cfg)) => Self::render_func_header_cs(funcs, cfg)?,
                TemplateKind::Func((func, cfg)) => Self::render_func_cs(func, cfg)?,
            };
            code_blocks.push(s);
        }
        Ok(code_blocks)
    }

    fn render_record_cs(def: UniRecordDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateRecordCS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render csharp record template error", e)
        })?;
        Ok(s)
    }

    fn render_table_cs(def: UniTableDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateTableCS::from(def, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render csharp table template error", e))?;
        Ok(s)
    }

    fn render_enum_cs(def: UniEnumDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateEnumCS::from(def, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render csharp enum template error", e))?;
        Ok(s)
    }

    fn render_variant_cs(def: UniVariantDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateVariantCS::from(def, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render csharp variant template error", e)
        })?;
        Ok(s)
    }

    fn render_entity_cs(table_schema: TableDef) -> RS<String> {
        let template = TemplateEntityCS::from_table_schema(&table_schema)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render csharp entity template error", e)
        })?;
        Ok(s)
    }

    fn render_func_header_cs(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncHeaderCS::from(funcs, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "render csharp func header template error",
                e
            )
        })?;
        Ok(s)
    }

    fn render_func_cs(func: WitFuncDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncCS::from(func, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render csharp func template error", e))?;
        Ok(s)
    }
}
