use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniTableDef, UniVariantDef};

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
        }
    }
}

impl Default for WitDef {
    fn default() -> WitDef {
        Self::new()
    }
}
