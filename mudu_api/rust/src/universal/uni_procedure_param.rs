use crate::universal::uni_oid::UniOid;

use crate::universal::uni_data_value::UniDataValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniProcedureParam {
    pub procedure: u64,

    pub session: UniOid,

    pub param_list: Vec<UniDataValue>,
}
