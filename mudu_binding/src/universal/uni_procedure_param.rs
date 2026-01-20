use crate::universal::uni_oid::UniOid;

use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniProcedureParam {
    pub procedure: u64,

    pub session: UniOid,

    pub param_list: Vec<UniDatValue>,
}

impl Default for UniProcedureParam {
    fn default() -> Self {
        Self {
            procedure: Default::default(),

            session: Default::default(),

            param_list: Default::default(),
        }
    }
}
