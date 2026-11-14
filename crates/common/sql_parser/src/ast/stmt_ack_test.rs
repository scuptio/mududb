//! Unit tests for `StmtAck`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::stmt_ack::StmtAck;

#[test]
fn ack_stores_name_and_seq() {
    let stmt = StmtAck::new("order_events".to_string(), 42);

    assert_eq!(stmt.name(), "order_events");
    assert_eq!(stmt.seq(), 42);
}

#[test]
fn ack_consumer_defaults_to_none() {
    let mut stmt = StmtAck::new("order_events".to_string(), 42);
    assert_eq!(stmt.consumer(), None);

    stmt.set_consumer("cg1".to_string());
    assert_eq!(stmt.consumer(), Some("cg1"));
}
