use crate::universal::uni_dat_type::UniDatType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A collection of uni data type description

/// [/tool/test_data/types.desc.json](/tool/test_data/types.desc.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniTypeDesc {
    pub types: HashMap<String, UniDatType>,
}

impl Default for UniTypeDesc {
    fn default() -> Self {
        UniTypeDesc {
            types: HashMap::default(),
        }
    }
}

impl UniTypeDesc {
    pub fn extend(&mut self, other: UniTypeDesc) {
        self.types.extend(other.types);
    }
}
