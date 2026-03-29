use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniKeyValue {
    pub key: UniDatValue,

    pub value: UniDatValue,
}

impl Default for UniKeyValue {
    fn default() -> Self {
        Self {
            key: Default::default(),
            value: Default::default(),
        }
    }
}
