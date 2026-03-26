use crate::src_gen::codegen_cfg::CodegenCfg;
use mudu_binding::record::record_def::RecordDef;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniTableDef, UniVariantDef};

pub enum TemplateKind {
    Enum((UniEnumDef, CodegenCfg)),
    Variant((UniVariantDef, CodegenCfg)),
    Record((UniRecordDef, CodegenCfg)),
    Table((UniTableDef, CodegenCfg)),
    Entity(RecordDef),
}
