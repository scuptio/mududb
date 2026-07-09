use crate::universal::uni_data_value::UniDataValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniSqlParam {
    pub params: Vec<UniDataValue>,
}
