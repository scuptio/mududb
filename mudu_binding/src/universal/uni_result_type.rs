use crate::universal::uni_data_type::UniDataType;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniResultType {
    pub ok: Option<Box<UniDataType>>,

    pub err: Option<Box<UniDataType>>,
}
