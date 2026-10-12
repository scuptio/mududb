//! Transaction abstraction.

use mudu::common::id::OID;

/// A database transaction.
pub trait Tx {
    /// Returns the transaction id.
    fn xid(&self) -> OID;
}
