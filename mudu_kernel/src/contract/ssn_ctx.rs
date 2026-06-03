use mudu::common::result::RS;
use mudu::common::id::OID;

/// session context
pub trait SsnCtx: Send + Sync {
    fn current_tx(&self) -> Option<OID>;

    fn begin_tx(&self, xid: OID) -> RS<()>;

    fn end_tx(&self) -> RS<()>;
}
