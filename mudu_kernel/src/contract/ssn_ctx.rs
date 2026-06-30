use mudu::common::id::OID;
use mudu::common::result::RS;

/// session context
pub trait SsnCtx: Send + Sync {
    fn current_tx(&self) -> Option<OID>;

    fn begin_tx(&self, xid: OID) -> RS<()>;

    fn end_tx(&self) -> RS<()>;
}
