use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniKeyValue {
    pub key: UniDatValue,

    pub value: UniDatValue,
}
