use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniRangeArgv {
    pub oid: UniOid,

    pub start_key: UniDatValue,

    pub end_key: UniDatValue,
}

impl Default for UniRangeArgv {
    fn default() -> Self {
        Self {
            oid: Default::default(),
            start_key: Default::default(),
            end_key: Default::default(),
        }
    }
}
