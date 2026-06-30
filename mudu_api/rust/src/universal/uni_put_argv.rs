use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniPutArgv {
    pub oid: UniOid,

    pub key: UniDatValue,

    pub value: UniDatValue,
}
