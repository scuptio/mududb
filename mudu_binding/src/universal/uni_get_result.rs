use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniGetResult {
    pub value: Option<UniDatValue>,
}

impl Default for UniGetResult {
    fn default() -> Self {
        Self { value: None }
    }
}
