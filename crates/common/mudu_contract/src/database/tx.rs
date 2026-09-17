//! `database::tx` module.
#![allow(missing_docs)]

use mudu::common::id::OID;

pub trait Tx {
    fn xid(&self) -> OID;
}
