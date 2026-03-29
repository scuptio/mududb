use crate::universal::uni_oid::UniOid;
use crate::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu::common::id::OID;

impl UniSessionOpenArgv {
    pub fn new(worker_id: OID) -> Self {
        Self {
            worker_id: UniOid::from(worker_id),
        }
    }

    pub fn worker_oid(&self) -> OID {
        self.worker_id.to_oid()
    }
}
