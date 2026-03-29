use crate::universal::uni_key_value::UniKeyValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniRangeResult {
    pub items: Vec<UniKeyValue>,
}

impl Default for UniRangeResult {
    fn default() -> Self {
        Self {
            items: Default::default(),
        }
    }
}
