//! Unit tests for `StmtCreateTable`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::column_def::ColumnDef;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_table_partition::StmtTablePartition;
use mudu::common::id::AttrIndex;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

fn int_column(name: &str) -> ColumnDef {
    ColumnDef::new(name.to_string(), UniDataType::Scalar(UniScalar::I32), None)
}

#[test]
fn new_stores_table_name() {
    let stmt = StmtCreateTable::new("users".to_string());
    assert_eq!(stmt.table_name(), "users");
    assert!(stmt.column_def().is_empty());
}

#[test]
fn add_column_def_assigns_index() {
    let mut stmt = StmtCreateTable::new("users".to_string());
    stmt.add_column_def(int_column("id"));
    stmt.add_column_def(int_column("name"));

    assert_eq!(stmt.column_def().len(), 2);
    assert_eq!(stmt.column_def()[0].column_index(), 0 as AttrIndex);
    assert_eq!(stmt.column_def()[1].column_index(), 1 as AttrIndex);
}

#[test]
fn mutable_column_def_allows_modification() {
    let mut stmt = StmtCreateTable::new("users".to_string());
    stmt.add_column_def(int_column("id"));
    stmt.mutable_column_def()[0].set_primary_key_index(Some(0));
    assert!(stmt.column_def()[0].is_primary_key());
}

#[test]
fn column_def_by_index_returns_expected_column() {
    let mut stmt = StmtCreateTable::new("users".to_string());
    stmt.add_column_def(int_column("id"));
    stmt.add_column_def(int_column("name"));
    assert_eq!(stmt.column_def_by_index(1).column_name(), "name");
}

#[test]
fn partition_accessor_and_mutator() {
    let mut stmt = StmtCreateTable::new("users".to_string());
    assert!(stmt.partition().is_none());

    let partition = StmtTablePartition::new("rule".to_string(), vec!["id".to_string()]);
    stmt.set_partition(partition);
    assert_eq!(stmt.partition().unwrap().rule_name(), "rule");
}

#[test]
fn assign_index_for_columns_separates_primary_and_non_primary() {
    let mut stmt = StmtCreateTable::new("users".to_string());
    let mut id_col = int_column("id");
    id_col.set_primary_key_index(Some(1));
    let name_col = int_column("name");

    stmt.add_column_def(id_col);
    stmt.add_column_def(name_col);
    stmt.assign_index_for_columns();

    assert_eq!(stmt.primary_column_indices(), &vec![0 as AttrIndex]);
    assert_eq!(stmt.non_primary_column_indices(), &vec![1 as AttrIndex]);

    let primary = stmt.primary_columns();
    let non_primary = stmt.non_primary_columns();
    assert_eq!(primary.len(), 1);
    assert_eq!(non_primary.len(), 1);
    assert_eq!(primary[0].column_name(), "id");
    assert_eq!(non_primary[0].column_name(), "name");
}
