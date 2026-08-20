use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniFsStat {
    pub oid: UniOid,

    pub generation: u64,

    pub entry: String,

    pub length: u64,

    pub state: u32,
}
