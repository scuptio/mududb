use crate::universal::uni_key_value::UniKeyValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct UniRangeResult {
    pub items: Vec<UniKeyValue>,
}

