//! Unit tests for the native game-backend procedures.

use crate::rust::procedure::{command, event};
use mududb::common::id::OID;

#[test]
fn command_echoes_message() {
    let xid: OID = 7;
    let msg = vec![1, 2, 3, 4];
    assert_eq!(command(xid, msg.clone()), Ok(msg));
}

#[test]
fn event_returns_empty() {
    let xid: OID = 8;
    assert_eq!(event(xid), Ok(Vec::new()));
}
