use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniPutArgv {
    pub oid: UniOid,

    pub key: UniDataValue,

    pub value: UniDataValue,
}
