use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodegenCfg {
    pub impl_inner_func:bool,
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

impl Default for CodegenCfg {
    fn default() -> Self {
        Self {
            impl_inner_func: false,
            impl_serialize: false,
            impl_default: false,
            impl_display: false,
            impl_from_str: false,
            impl_eq: false,
            impl_hash: false,
        }
    }
}

