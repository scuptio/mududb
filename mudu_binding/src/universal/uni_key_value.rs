use crate::universal::uni_data_value::UniDataValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniKeyValue {
    pub key: UniDataValue,

    pub value: UniDataValue,
}
