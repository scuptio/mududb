//! Unit tests for `StmtReceive`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::stmt_receive::{ReceiveWait, StmtReceive};

#[test]
fn receive_stores_name_count_and_wait() {
    let stmt = StmtReceive::new(
        "order_events".to_string(),
        Some(10),
        Some(ReceiveWait::Timeout(30000)),
    );

    assert_eq!(stmt.name(), "order_events");
    assert_eq!(stmt.count(), Some(10));
    assert_eq!(stmt.wait(), Some(ReceiveWait::Timeout(30000)));
}

#[test]
fn receive_defaults_are_absent() {
    let stmt = StmtReceive::new("q".to_string(), None, None);

    assert_eq!(stmt.count(), None);
    assert_eq!(stmt.wait(), None);
}

#[test]
fn receive_wait_forever() {
    let stmt = StmtReceive::new("q".to_string(), Some(1), Some(ReceiveWait::Forever));
    assert_eq!(stmt.wait(), Some(ReceiveWait::Forever));
}

#[test]
fn receive_consumer_defaults_to_none() {
    let mut stmt = StmtReceive::new("q".to_string(), None, None);
    assert_eq!(stmt.consumer(), None);

    stmt.set_consumer("cg1".to_string());
    assert_eq!(stmt.consumer(), Some("cg1"));
}
