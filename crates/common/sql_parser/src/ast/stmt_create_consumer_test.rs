//! Unit tests for `StmtCreateConsumer`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::stmt_create_consumer::StmtCreateConsumer;

#[test]
fn create_consumer_stores_consumer_and_queue() {
    let stmt = StmtCreateConsumer::new("cg1".to_string(), "order_events".to_string());

    assert_eq!(stmt.consumer(), "cg1");
    assert_eq!(stmt.queue(), "order_events");
}
