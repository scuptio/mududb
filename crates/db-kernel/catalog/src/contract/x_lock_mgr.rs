use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use mudu_sys::sync::async_::notify_wait::Notify;

/// Outcome of a lock acquisition, delivered through the notify handle.
#[derive(Debug, Clone)]
pub enum LockResult {
    /// The lock was acquired.
    Locked,
    /// The lock could not be acquired.
    LockFailed,
}

/// Transaction key-lock manager: per-table lock namespaces with asynchronous
/// lock acquisition.
#[async_trait]
pub trait XLockMgr: Send + Sync {
    /// Creates the lock namespace for `table`, with keys shaped by
    /// `tuple_desc`.
    async fn create_table(&self, table: OID, tuple_desc: TupleDesc) -> RS<()>;

    /// Drops the lock namespace of `table`.
    async fn drop_table(&self, table: OID) -> RS<()>;

    /// Acquires the lock on `key` of table `table_id` for transaction `xid`,
    /// delivering the outcome through `notify`.
    async fn lock(&self, notify: Notify<LockResult>, xid: OID, table_id: OID, key: Buf) -> RS<()>;

    /// Releases the lock on `key` of table `table_id` held by transaction
    /// `xid`.
    async fn release(&self, xid: OID, table_id: OID, key: &Buf) -> RS<()>;
}
