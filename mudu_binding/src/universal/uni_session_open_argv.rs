use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniSessionOpenArgv {
    pub worker_id: UniOid,
}
