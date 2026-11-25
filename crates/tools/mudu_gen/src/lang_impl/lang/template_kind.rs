//! Kinds of template elements that can be rendered.

use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use mudu_binding::table::class_def::ClassDef;
use mudu_binding::universal::uni_def::{UniClassDef, UniEnumDef, UniStructDef, UniVariantDef};

/// A single top-level element inside an `AbstractTemplate`.
pub enum TemplateKind {
    /// Enum definition and its generation config.
    Enum((UniEnumDef, CodegenCfg)),
    /// Variant definition and its generation config.
    Variant((UniVariantDef, CodegenCfg)),
    /// Struct definition and its generation config.
    Struct((UniStructDef, CodegenCfg)),
    /// Table definition and its generation config.
    Table((UniClassDef, CodegenCfg)),
    /// Object derived from a SQL table definition.
    Object(ClassDef),
    /// Shared MSSP frame codec preamble (message kinds plus frame helpers)
    /// for all functions of one WIT file.
    FuncHeader((Vec<WitFuncDef>, CodegenCfg)),
    /// Per-function MSSP request/result codec stubs.
    Func((WitFuncDef, CodegenCfg)),
}
