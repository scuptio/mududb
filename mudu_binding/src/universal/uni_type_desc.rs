use crate::universal::uni_data_type::UniDataType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A collection of uni data type description
/// [/tool/test_data/types.desc.json](/tool/test_data/types.desc.json)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UniTypeDesc {
    pub types: HashMap<String, UniDataType>,
}

impl UniTypeDesc {
    pub fn extend(&mut self, other: UniTypeDesc) {
        self.types.extend(other.types);
    }
}
