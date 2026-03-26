use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniSqlParam {
    pub params: Vec<UniDatValue>,
}

impl Default for UniSqlParam {
    fn default() -> Self {
        Self {
            params: Default::default(),
        }
    }
}
