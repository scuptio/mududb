//! Unit tests for `StmtDropQueue`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::stmt_drop_queue::StmtDropQueue;

#[test]
fn drop_queue_stores_name() {
    let stmt = StmtDropQueue::new("order_events".to_string());
    assert_eq!(stmt.name(), "order_events");
}
