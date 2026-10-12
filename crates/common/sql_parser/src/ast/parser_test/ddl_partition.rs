#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;
use project_root::get_project_root;

use std::path::Path;

fn parse_file<P: AsRef<Path>>(path: P) -> RS<Vec<StmtType>> {
    let sql = mudu_sys::fs::sync::sync_read_to_string(path).unwrap();
    parse_sql(&sql)
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_create_table_ast_column_primary_key_index() {
    let stmt = parse_create_table(
        "
        CREATE TABLE Persons (
            PersonID int PRIMARY KEY,
            LastName char(255),
            FirstName char(255)
        );
        ",
    )
    .unwrap();

    let primary_columns = stmt.primary_columns();
    assert_eq!(primary_columns.len(), 1);
    assert_eq!(primary_columns[0].column_name(), "PersonID");
    assert_eq!(primary_columns[0].primary_key_index(), Some(0));
    assert_eq!(primary_columns[0].column_index(), 0);

    let non_primary_columns = stmt.non_primary_columns();
    assert_eq!(non_primary_columns.len(), 2);
    assert_eq!(non_primary_columns[0].column_name(), "LastName");
    assert_eq!(non_primary_columns[0].primary_key_index(), None);
    assert_eq!(non_primary_columns[0].column_index(), 1);
    assert_eq!(non_primary_columns[1].column_name(), "FirstName");
    assert_eq!(non_primary_columns[1].primary_key_index(), None);
    assert_eq!(non_primary_columns[1].column_index(), 2);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_create_table_ast_table_primary_key_index_and_idempotent() {
    let mut stmt = parse_create_table(
        "
        CREATE TABLE CUSTOMERS(
            ID1 INT,
            ID2 INT,
            NAME CHAR(20),
            PRIMARY KEY (ID1, ID2)
        );
        ",
    )
    .unwrap();

    stmt.assign_index_for_columns();

    let primary_columns = stmt.primary_columns();
    assert_eq!(primary_columns.len(), 2);
    assert_eq!(primary_columns[0].column_name(), "ID1");
    assert_eq!(primary_columns[0].primary_key_index(), Some(0));
    assert_eq!(primary_columns[0].column_index(), 0);
    assert_eq!(primary_columns[1].column_name(), "ID2");
    assert_eq!(primary_columns[1].primary_key_index(), Some(1));
    assert_eq!(primary_columns[1].column_index(), 1);

    let non_primary_columns = stmt.non_primary_columns();
    assert_eq!(non_primary_columns.len(), 1);
    assert_eq!(non_primary_columns[0].column_name(), "NAME");
    assert_eq!(non_primary_columns[0].primary_key_index(), None);
    assert_eq!(non_primary_columns[0].column_index(), 2);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_parse_ddl_file() {
    let path = get_project_root().unwrap();
    let path = if path.file_name().unwrap().to_str().unwrap().eq("sql_parser") {
        path
    } else {
        path.join("sql_parser")
    };
    let path = path.join("data/ddl.sql");
    let r = parse_file(path);
    assert!(r.is_ok())
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_partition_rule_custom_statement() {
    let stmts = parse_sql(
        "
        CREATE PARTITION RULE r_orders RANGE (
            PARTITION p0 VALUES FROM (MINVALUE, MINVALUE) TO (1000, MINVALUE),
            PARTITION p1 VALUES FROM (1000, MINVALUE) TO (MAXVALUE, MAXVALUE)
        );
        ",
    )
    .unwrap();

    let StmtType::Command(StmtCommand::CreatePartitionRule(stmt)) = &stmts[0] else {
        panic!("expected create partition rule");
    };
    assert_eq!(stmt.rule_name(), "r_orders");
    assert_eq!(stmt.partitions().len(), 2);
    assert_eq!(stmt.partitions()[0].name(), "p0");
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_with_partition_binding_clause() {
    let stmt = parse_create_table(
        "
        CREATE TABLE orders (
            region_id INT,
            order_id INT,
            amount INT,
            PRIMARY KEY (region_id, order_id)
        )
        PARTITION BY GLOBAL RULE r_orders REFERENCES (region_id, order_id);
        ",
    )
    .unwrap();

    let partition = stmt.partition().expect("expected partition binding");
    assert_eq!(partition.rule_name(), "r_orders");
    assert_eq!(
        partition.reference_columns(),
        &vec!["region_id".to_string(), "order_id".to_string()]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_partition_placement_custom_statement() {
    let stmts = parse_sql(
        "
        CREATE PARTITION PLACEMENT FOR RULE r_orders (
            PARTITION p0 ON WORKER 11,
            PARTITION p1 ON WORKER 12
        );
        ",
    )
    .unwrap();

    let StmtType::Command(StmtCommand::CreatePartitionPlacement(stmt)) = &stmts[0] else {
        panic!("expected create partition placement");
    };
    assert_eq!(stmt.rule_name(), "r_orders");
    assert_eq!(stmt.placements().len(), 2);
    assert_eq!(stmt.placements()[0].partition_name(), "p0");
    assert_eq!(stmt.placements()[0].worker_id(), "11");
}
