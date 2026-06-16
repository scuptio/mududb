use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct CodegenCfg {
    pub impl_inner_func: bool,
    pub impl_serialize: bool,
    pub impl_default: bool,
    pub impl_display: bool,
    pub impl_from_str: bool,
    pub impl_eq: bool,
    pub impl_hash: bool,
}

impl CodegenCfg {
    pub fn new() -> CodegenCfg {
        Default::default()
    }
}

