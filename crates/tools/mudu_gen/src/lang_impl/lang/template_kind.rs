//! Kinds of template elements that can be rendered.

use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use mudu_binding::table::table_def::TableDef;
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
    Entity(TableDef),
    /// Shared MSSP frame codec preamble (message kinds plus frame helpers)
    /// for all functions of one WIT file.
    FuncHeader((Vec<WitFuncDef>, CodegenCfg)),
    /// Per-function MSSP request/result codec stubs.
    Func((WitFuncDef, CodegenCfg)),
}
