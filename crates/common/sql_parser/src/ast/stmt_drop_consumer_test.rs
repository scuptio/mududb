//! Unit tests for `StmtDropConsumer`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::stmt_drop_consumer::StmtDropConsumer;

#[test]
fn drop_consumer_stores_consumer_and_queue() {
    let stmt = StmtDropConsumer::new("cg1".to_string(), "order_events".to_string());

    assert_eq!(stmt.consumer(), "cg1");
    assert_eq!(stmt.queue(), "order_events");
}
