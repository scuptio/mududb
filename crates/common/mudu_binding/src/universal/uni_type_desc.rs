use crate::universal::uni_data_type::UniDataType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A collection of uni data type description
/// [/tool/test_data/types.desc.json](/tool/test_data/types.desc.json)
///
/// The map is a `BTreeMap` (not a `HashMap`) so serialization emits keys in
/// sorted order: regenerating a `type.desc.json` from unchanged inputs must
/// be byte-identical.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UniTypeDesc {
    pub types: BTreeMap<String, UniDataType>,
}

impl UniTypeDesc {
    pub fn extend(&mut self, other: UniTypeDesc) {
        self.types.extend(other.types);
    }
}
