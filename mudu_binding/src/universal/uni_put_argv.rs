use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniPutArgv {
    pub oid: UniOid,

    pub key: UniDatValue,

    pub value: UniDatValue,
}

impl Default for UniPutArgv {
    fn default() -> Self {
        Self {
            oid: Default::default(),
            key: Default::default(),
            value: Default::default(),
        }
    }
}
