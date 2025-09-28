use mudu::common::result::RS;
use mudu::common::xid::XID;

/// session context
pub trait SsnCtx: Send + Sync {
    fn current_tx(&self) -> Option<XID>;

    fn begin_tx(&self, xid: XID) -> RS<()>;

    fn end_tx(&self) -> RS<()>;
}
