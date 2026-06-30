#![allow(clippy::unwrap_used)]
#![allow(clippy::panic)]

use crate::ast::parser::SQLParser;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use mudu::error::ErrorCode;

fn parse(sql: &str) -> crate::ast::stmt_list::StmtList {
    SQLParser::new().unwrap().parse(sql).unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn empty_and_whitespace_custom_statements_yield_empty_list() {
    assert!(parse("").stmts().is_empty());
    assert!(parse("   ").stmts().is_empty());
    assert!(parse(";").stmts().is_empty());
    assert!(parse("  ;  ").stmts().is_empty());
}

#[test]
#[cfg_attr(miri, ignore)]
fn create_partition_rule_success_and_errors() {
    let sql = "create partition rule sales range (\
        partition p1 values from (minvalue) to (100), \
        partition p2 values from (100) to (maxvalue));";
    let stmt = parse(sql).stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::CreatePartitionRule(rule)) = stmt else {
        panic!("expected create partition rule");
    };
    assert_eq!(rule.rule_name(), "sales");
    assert_eq!(rule.partitions().len(), 2);

    let bad = SQLParser::new()
        .unwrap()
        .parse("create partition rule sales range;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    let bad = SQLParser::new()
        .unwrap()
        .parse("create partition rule range (partition p1 values from (minvalue) to (maxvalue));");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    let bad = SQLParser::new().unwrap().parse(
        "create partition rule sales range partition p1 values from (minvalue) to (maxvalue);",
    );
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn create_partition_placement_success_and_errors() {
    let sql = "create partition placement for rule sales (partition p1 on worker node1, partition p2 on worker node2);";
    let stmt = parse(sql).stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::CreatePartitionPlacement(placement)) = stmt else {
        panic!("expected create partition placement");
    };
    assert_eq!(placement.rule_name(), "sales");
    assert_eq!(placement.placements().len(), 2);

    let bad = SQLParser::new()
        .unwrap()
        .parse("create partition placement for sales (partition p1 on worker node1);");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    let bad = SQLParser::new()
        .unwrap()
        .parse("create partition placement for rule sales;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    let bad = SQLParser::new()
        .unwrap()
        .parse("create partition placement for rule sales ();");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    let bad = SQLParser::new()
        .unwrap()
        .parse("create partition placement for rule  (partition p1 on worker node1);");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn create_table_partitioned_success_and_errors() {
    let sql = "create table t (id int) partition by global rule sales references (id);";
    let stmt = parse(sql).stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::CreateTable(table)) = stmt else {
        panic!("expected create table");
    };
    assert!(table.partition().is_some());

    // No column list at all -> covers the "partitioned create table has no column list" branch.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create table t partition by global rule sales references id;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn invalid_standard_sql_returns_parse_error() {
    let err = SQLParser::new()
        .unwrap()
        .parse("select * fro;")
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::MlParse);
}
