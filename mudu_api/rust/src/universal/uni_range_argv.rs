use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniRangeArgv {
    pub oid: UniOid,

    pub start_key: UniDatValue,

    pub end_key: UniDatValue,
}
