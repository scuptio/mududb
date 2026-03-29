use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniGetArgv {
    pub oid: UniOid,

    pub key: UniDatValue,
}

impl Default for UniGetArgv {
    fn default() -> Self {
        Self {
            oid: Default::default(),
            key: Default::default(),
        }
    }
}
