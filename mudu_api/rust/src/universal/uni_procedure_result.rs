use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniProcedureResult {
    pub return_list: Vec<UniDatValue>,
}

impl Default for UniProcedureResult {
    fn default() -> Self {
        Self {
            return_list: Default::default(),
        }
    }
}
