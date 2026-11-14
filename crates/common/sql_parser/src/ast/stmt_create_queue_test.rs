//! Unit tests for `StmtCreateQueue`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::column_def::ColumnDef;
use crate::ast::stmt_create_queue::{QueuePayload, StmtCreateQueue};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

#[test]
fn create_queue_stores_like_payload() {
    let stmt = StmtCreateQueue::new("q".to_string(), QueuePayload::Like("t".to_string()));

    assert_eq!(stmt.name(), "q");
    let QueuePayload::Like(table) = stmt.payload() else {
        panic!("expected LIKE payload");
    };
    assert_eq!(table, "t");
}

#[test]
fn create_queue_stores_column_payload() {
    let columns = vec![
        ColumnDef::new("id".to_string(), UniDataType::Scalar(UniScalar::I32), None),
        ColumnDef::new(
            "body".to_string(),
            UniDataType::Scalar(UniScalar::String),
            None,
        ),
    ];
    let stmt = StmtCreateQueue::new("q".to_string(), QueuePayload::Columns(columns));

    assert_eq!(stmt.name(), "q");
    let QueuePayload::Columns(columns) = stmt.payload() else {
        panic!("expected column payload");
    };
    assert_eq!(columns.len(), 2);
    assert_eq!(columns[0].column_name(), "id");
    assert_eq!(columns[1].column_name(), "body");
}
