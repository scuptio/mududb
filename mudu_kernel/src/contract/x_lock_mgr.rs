use crate::sync::notify_wait::Notify;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;

#[derive(Debug, Clone)]
pub enum LockResult {
    Locked,
    LockFailed,
}

#[async_trait]
pub trait XLockMgr: Send + Sync {
    async fn create_table(&self, table: OID, tuple_desc: TupleDesc) -> RS<()>;

    async fn drop_table(&self, table: OID) -> RS<()>;

    async fn lock(&self, notify: Notify<LockResult>, xid: XID, table_id: OID, key: Buf) -> RS<()>;

    async fn release(&self, xid: XID, table_id: OID, key: &Buf) -> RS<()>;
}
