//! Kinds of template elements that can be rendered.

use crate::src_gen::codegen_cfg::CodegenCfg;
use mudu_binding::record::record_def::RecordDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniTableDef, UniVariantDef};

/// A single top-level element inside an [`AbstractTemplate`](crate::lang_impl::lang::abstract_template::AbstractTemplate).
pub enum TemplateKind {
    /// Enum definition and its generation config.
    Enum((UniEnumDef, CodegenCfg)),
    /// Variant definition and its generation config.
    Variant((UniVariantDef, CodegenCfg)),
    /// Record definition and its generation config.
    Record((UniRecordDef, CodegenCfg)),
    /// Table definition and its generation config.
    Table((UniTableDef, CodegenCfg)),
    /// Entity derived from a SQL table definition.
    Entity(RecordDef),
}
