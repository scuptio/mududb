use crate::universal::uni_data_type::UniDataType;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniFieldAttr {
    pub attr_name: String,

    pub attr_value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniRecordField {
    pub field_name: String,

    pub field_type: UniDataType,

    #[serde(default)]
    pub field_attrs: Vec<UniFieldAttr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniRecordType {
    pub record_name: String,

    pub record_fields: Vec<UniRecordField>,
}
