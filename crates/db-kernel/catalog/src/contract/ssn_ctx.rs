use mudu::common::id::OID;
use mudu::common::result::RS;

/// session context
pub trait SsnCtx: Send + Sync {
    /// The xid of the transaction currently bound to the session, if any.
    fn current_tx(&self) -> Option<OID>;

    /// Binds transaction `xid` to the session.
    fn begin_tx(&self, xid: OID) -> RS<()>;

    /// Unbinds the current transaction from the session.
    fn end_tx(&self) -> RS<()>;
}
