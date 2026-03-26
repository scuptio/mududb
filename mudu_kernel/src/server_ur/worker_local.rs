use crate::storage::worker_kv_store::KvItem;
use mudu::common::id::OID;
use mudu::common::result::RS;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerExecute {
    BeginTx,
    CommitTx,
    RollbackTx,
}

pub trait WorkerLocal: Send + Sync {
    fn open(&self) -> RS<OID>;

    fn close(&self, session_id: OID) -> RS<()>;

    fn execute(&self, session_id: OID, instruction: WorkerExecute) -> RS<()>;

    fn put(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()>;

    fn get(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>>;

    fn range(&self, session_id: OID, start_key: &[u8], end_key: &[u8]) -> RS<Vec<KvItem>>;
}

pub type WorkerLocalRef = Arc<dyn WorkerLocal + Send + Sync>;
