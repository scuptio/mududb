use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniFsOpenArgv {
    pub session: UniOid,

    pub oid: UniOid,

    pub path: String,

    pub flags: u32,
}
