use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniVariantDef};

#[derive(Debug, Clone)]
pub struct WitDef {
    pub interface:Vec<String>,
    pub use_path:Vec<Vec<String>>,
    pub records: Vec<UniRecordDef>,
    pub variants: Vec<UniVariantDef>,
    pub enums: Vec<UniEnumDef>,
}

impl WitDef {
    fn new() -> WitDef {
        Self {
            interface: vec![],
            use_path: vec![],
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