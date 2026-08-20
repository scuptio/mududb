#![allow(clippy::unwrap_used)]
#![allow(clippy::panic)]

use crate::ast::parser::SQLParser;
use crate::ast::stmt_create_fs_type::FsTypeKind;
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
fn create_type_filesystem_success_and_errors() {
    let stmt = parse("create type filesystem file wal_log;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateFsType(create)) = stmt else {
        panic!("expected create type filesystem");
    };
    assert_eq!(create.name(), "wal_log");
    assert_eq!(create.kind(), FsTypeKind::File);

    let stmt = parse("CREATE TYPE FILESYSTEM DIRECTORY backup_dir;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateFsType(create)) = stmt else {
        panic!("expected create type filesystem");
    };
    assert_eq!(create.name(), "backup_dir");
    assert_eq!(create.kind(), FsTypeKind::Directory);

    // Unknown kind keyword.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create type filesystem volume v1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Missing type name.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create type filesystem file;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Trailing garbage after the type name.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create type filesystem file v1 extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Type name must not start with a digit.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create type filesystem file 1abc;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn drop_type_success_and_errors() {
    let stmt = parse("drop type wal_log;").stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::DropType(drop)) = stmt else {
        panic!("expected drop type");
    };
    assert_eq!(drop.name(), "wal_log");

    // Missing type name.
    let bad = SQLParser::new().unwrap().parse("drop type;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Trailing garbage after the type name.
    let bad = SQLParser::new().unwrap().parse("drop type wal_log extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Type name must not start with a digit.
    let bad = SQLParser::new().unwrap().parse("drop type 1abc;");
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

#[test]
#[cfg_attr(miri, ignore)]
fn select_aggregate_functions_parse() {
    use crate::ast::expr_function::FunctionArg;
    use crate::ast::select_term::SelectField;

    let stmt = parse("select count(*) as c, sum(amount) from stock;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Select(select) = stmt else {
        panic!("expected select");
    };
    let terms = select.get_select_term_list();
    assert_eq!(terms.len(), 2);
    match terms[0].field() {
        SelectField::Function(f) => {
            assert_eq!(f.name(), "count");
            assert!(matches!(f.arg(), FunctionArg::Star));
        }
        SelectField::Column(_) => panic!("expected function field"),
    }
    assert_eq!(terms[0].alias(), "c");
    match terms[1].field() {
        SelectField::Function(f) => {
            assert_eq!(f.name(), "sum");
            match f.arg() {
                FunctionArg::Column(name) => assert_eq!(name.name(), "amount"),
                FunctionArg::Star => panic!("expected column argument"),
            }
        }
        SelectField::Column(_) => panic!("expected function field"),
    }

    // DISTINCT inside an invocation is rejected as not implemented.
    let err = SQLParser::new()
        .unwrap()
        .parse("select count(distinct a) from stock;")
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);

    // An arbitrary expression argument is rejected as not implemented.
    let err = SQLParser::new()
        .unwrap()
        .parse("select sum(a + 1) from stock;")
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
}

#[test]
#[cfg_attr(miri, ignore)]
fn mixed_script_with_partition_ddl_parses() {
    let sql = "-- a comment line\n\
        CREATE PARTITION RULE r_wh RANGE (PARTITION p1 VALUES FROM (1) TO (2), PARTITION p2 VALUES FROM (2) TO (3));\n\
        CREATE TABLE stock (\n\
            s_w_id INT,\n\
            s_i_id INT,\n\
            PRIMARY KEY (s_w_id, s_i_id)\n\
        ) PARTITION BY GLOBAL RULE r_wh REFERENCES (s_w_id);\n\
        CREATE TABLE plain_t (id INT PRIMARY KEY, v TEXT);\n";
    let list = parse(sql);
    assert_eq!(list.stmts().len(), 3);
    let StmtType::Command(StmtCommand::CreatePartitionRule(rule)) = &list.stmts()[0] else {
        panic!("expected create partition rule");
    };
    assert_eq!(rule.rule_name(), "r_wh");
    let StmtType::Command(StmtCommand::CreateTable(table)) = &list.stmts()[1] else {
        panic!("expected partitioned create table");
    };
    assert!(table.partition().is_some());
    let StmtType::Command(StmtCommand::CreateTable(table)) = &list.stmts()[2] else {
        panic!("expected plain create table");
    };
    assert!(table.partition().is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn mixed_script_respects_semicolons_in_strings() {
    let sql = "CREATE TABLE t (id INT PRIMARY KEY, v TEXT);\n\
               INSERT INTO t VALUES (1, 'a;b');";
    let list = parse(sql);
    assert_eq!(list.stmts().len(), 2);
}
