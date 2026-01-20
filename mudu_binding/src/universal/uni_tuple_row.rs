use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniTupleRow {
    pub fields: Vec<UniDatValue>,
}

impl Default for UniTupleRow {
    fn default() -> Self {
        Self {
            fields: Default::default(),
        }
    }
}
