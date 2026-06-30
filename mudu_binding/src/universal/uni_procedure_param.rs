use crate::universal::uni_oid::UniOid;

use crate::universal::uni_dat_value::UniDatValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniProcedureParam {
    pub procedure: u64,

    pub session: UniOid,

    pub param_list: Vec<UniDatValue>,
}
