use mudu_binding::universal::uni_def::{
    RecordField, UniEnumDef, UniRecordDef, UniTableDef, UniVariantDef,
};

/// Parsed contents of a WIT interface file.
#[derive(Debug, Clone)]
pub struct WitDef {
    /// Interfaces declared in the file.
    pub interface: Vec<String>,
    /// Use/import paths declared at the top level.
    pub use_path: Vec<Vec<String>>,
    /// Table definitions.
    pub tables: Vec<UniTableDef>,
    /// Record definitions.
    pub records: Vec<UniRecordDef>,
    /// Variant definitions.
    pub variants: Vec<UniVariantDef>,
    /// Enum definitions.
    pub enums: Vec<UniEnumDef>,
    /// Function declarations inside interfaces.
    pub functions: Vec<WitFuncDef>,
}

impl WitDef {
    fn new() -> WitDef {
        Self {
            interface: vec![],
            use_path: vec![],
            tables: vec![],
            records: vec![],
            variants: vec![],
            enums: vec![],
            functions: vec![],
        }
    }
}

/// A WIT function declaration.
#[derive(Debug, Clone)]
pub struct WitFuncDef {
    /// Doc comments attached to the function.
    pub func_comments: String,
    /// Function name as declared in WIT (kebab-case).
    pub func_name: String,
    /// Named parameters.
    pub params: Vec<RecordField>,
    /// Named return values. Empty means the function returns nothing.
    pub returns: Vec<RecordField>,
}

impl Default for WitDef {
    fn default() -> WitDef {
        Self::new()
    }
}
