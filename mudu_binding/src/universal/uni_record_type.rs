use crate::universal::uni_dat_type::UniDatType;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniRecordField {
    pub field_name: String,

    pub field_type: UniDatType,
}

impl Default for UniRecordField {
    fn default() -> Self {
        Self {
            field_name: Default::default(),

            field_type: Default::default(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniRecordType {
    pub record_name: String,

    pub record_fields: Vec<UniRecordField>,
}

impl Default for UniRecordType {
    fn default() -> Self {
        Self {
            record_name: Default::default(),

            record_fields: Default::default(),
        }
    }
}
