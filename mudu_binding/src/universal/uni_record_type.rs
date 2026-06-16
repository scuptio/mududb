use crate::universal::uni_dat_type::UniDatType;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct UniRecordField {
    pub field_name: String,

    pub field_type: UniDatType,
}


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct UniRecordType {
    pub record_name: String,

    pub record_fields: Vec<UniRecordField>,
}

