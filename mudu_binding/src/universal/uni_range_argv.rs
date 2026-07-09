use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniRangeArgv {
    pub oid: UniOid,

    pub start_key: UniDataValue,

    pub end_key: UniDataValue,
}
