//! Unit tests for wallet procedure input validation.

use crate::rust::procedures::transfer_funds;
use mududb::common::id::OID;

#[test]
fn transfer_funds_rejects_non_positive_amount() {
    let xid: OID = 0;
    assert!(matches!(
        transfer_funds(xid, 1, 2, 0),
        Err(e) if e.message().contains("greater than 0")
    ));
}

#[test]
fn transfer_funds_rejects_self_transfer() {
    let xid: OID = 0;
    assert!(matches!(
        transfer_funds(xid, 1, 1, 100),
        Err(e) if e.message().contains("oneself")
    ));
}
