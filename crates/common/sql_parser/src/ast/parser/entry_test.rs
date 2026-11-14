#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::parser::SQLParser;
use crate::ast::stmt_create_fs_type::FsTypeKind;
use crate::ast::stmt_create_queue::QueuePayload;
use crate::ast::stmt_receive::ReceiveWait;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use mudu::error::ErrorCode;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

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
fn create_queue_like_success_and_errors() {
    let stmt = parse("create queue order_events like source_table;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateQueue(create)) = stmt else {
        panic!("expected create queue");
    };
    assert_eq!(create.name(), "order_events");
    let QueuePayload::Like(table) = create.payload() else {
        panic!("expected LIKE payload");
    };
    assert_eq!(table, "source_table");

    // Case-insensitive keywords and extra whitespace around the tokens.
    let stmt = parse("CREATE QUEUE   q1   LIKE   t1;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateQueue(create)) = stmt else {
        panic!("expected create queue");
    };
    assert_eq!(create.name(), "q1");
    let QueuePayload::Like(table) = create.payload() else {
        panic!("expected LIKE payload");
    };
    assert_eq!(table, "t1");

    // Missing LIKE or column list.
    let bad = SQLParser::new().unwrap().parse("create queue q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Queue name must not start with a digit.
    let bad = SQLParser::new().unwrap().parse("create queue 1q like t;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // LIKE without a table name.
    let bad = SQLParser::new().unwrap().parse("create queue q like;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // LIKE keyword must stand alone (`liket` is not `LIKE t`).
    let bad = SQLParser::new().unwrap().parse("create queue q liket;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Trailing garbage after the table name.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create queue q like t extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn create_queue_columns_success_and_errors() {
    let stmt = parse("create queue order_events (id int, body text);")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateQueue(create)) = stmt else {
        panic!("expected create queue");
    };
    assert_eq!(create.name(), "order_events");
    let QueuePayload::Columns(columns) = create.payload() else {
        panic!("expected column payload");
    };
    assert_eq!(columns.len(), 2);
    assert_eq!(columns[0].column_name(), "id");
    assert!(matches!(
        columns[0].data_type(),
        UniDataType::Scalar(UniScalar::I32)
    ));
    assert_eq!(columns[1].column_name(), "body");
    assert!(matches!(
        columns[1].data_type(),
        UniDataType::Scalar(UniScalar::String)
    ));

    // No space between the name and the column list is accepted.
    let stmt = parse("create queue q(id int);")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateQueue(create)) = stmt else {
        panic!("expected create queue");
    };
    assert_eq!(create.name(), "q");

    // Trailing text after the column list.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create queue q (id int) extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Unbalanced column list parentheses.
    let bad = SQLParser::new().unwrap().parse("create queue q (id int;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Malformed column definitions are rejected by the standard parser.
    let bad = SQLParser::new().unwrap().parse("create queue q (id int,);");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::MlParse);

    // An empty column list is rejected.
    let bad = SQLParser::new().unwrap().parse("create queue q ();");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::MlParse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn create_queue_partition_clause_success_and_errors() {
    // Column-list form with the sharding clause.
    let stmt =
        parse("create queue q (id int, body text) partition by global rule r references (id);")
            .stmts()
            .first()
            .unwrap()
            .clone();
    let StmtType::Command(StmtCommand::CreateQueue(create)) = stmt else {
        panic!("expected create queue");
    };
    let partition = create.partition().expect("partition clause parsed");
    assert_eq!(partition.rule_name(), "r");
    assert_eq!(partition.reference_columns(), &["id".to_string()]);

    // LIKE form with the sharding clause.
    let stmt = parse("create queue q like t partition by global rule r2 references (id);")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateQueue(create)) = stmt else {
        panic!("expected create queue");
    };
    let QueuePayload::Like(table) = create.payload() else {
        panic!("expected LIKE payload");
    };
    assert_eq!(table, "t");
    assert_eq!(
        create
            .partition()
            .expect("partition clause parsed")
            .rule_name(),
        "r2"
    );

    // No clause: unpartitioned queue.
    let stmt = parse("create queue q (id int);")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateQueue(create)) = stmt else {
        panic!("expected create queue");
    };
    assert!(create.partition().is_none());

    // A trailing clause that is not PARTITION BY GLOBAL RULE still errors.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create queue q (id int) partition by hash (id);");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
    let bad = SQLParser::new()
        .unwrap()
        .parse("create queue q like t partition by global rule r references ();");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
    let bad = SQLParser::new()
        .unwrap()
        .parse("create queue q like t something else;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn drop_queue_success_and_errors() {
    let stmt = parse("drop queue order_events;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::DropQueue(drop)) = stmt else {
        panic!("expected drop queue");
    };
    assert_eq!(drop.name(), "order_events");

    let stmt = parse("DROP QUEUE q1;").stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::DropQueue(drop)) = stmt else {
        panic!("expected drop queue");
    };
    assert_eq!(drop.name(), "q1");

    // Missing queue name.
    let bad = SQLParser::new().unwrap().parse("drop queue;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Trailing garbage after the queue name.
    let bad = SQLParser::new().unwrap().parse("drop queue q1 extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Queue name must not start with a digit.
    let bad = SQLParser::new().unwrap().parse("drop queue 1q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn receive_success() {
    // With an explicit count and no WAIT clause.
    let stmt = parse("receive 10 from order_events;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Receive(receive)) = stmt else {
        panic!("expected receive");
    };
    assert_eq!(receive.name(), "order_events");
    assert_eq!(receive.count(), Some(10));
    assert_eq!(receive.wait(), None);

    // Without a count.
    let stmt = parse("receive from order_events;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Receive(receive)) = stmt else {
        panic!("expected receive");
    };
    assert_eq!(receive.name(), "order_events");
    assert_eq!(receive.count(), None);
    assert_eq!(receive.wait(), None);

    // WAIT milliseconds, case-insensitive keywords, extra whitespace.
    let stmt = parse("RECEIVE  3  FROM  q1  WAIT  30000;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Receive(receive)) = stmt else {
        panic!("expected receive");
    };
    assert_eq!(receive.count(), Some(3));
    assert_eq!(receive.wait(), Some(ReceiveWait::Timeout(30000)));

    // WAIT FOREVER.
    let stmt = parse("receive 1 from q1 wait forever;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Receive(receive)) = stmt else {
        panic!("expected receive");
    };
    assert_eq!(receive.wait(), Some(ReceiveWait::Forever));
}

#[test]
#[cfg_attr(miri, ignore)]
fn receive_errors() {
    // Missing queue name.
    let bad = SQLParser::new().unwrap().parse("receive from;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Empty statement body.
    let bad = SQLParser::new().unwrap().parse("receive;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Non-numeric count.
    let bad = SQLParser::new().unwrap().parse("receive x from q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Negative count.
    let bad = SQLParser::new().unwrap().parse("receive -1 from q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Missing FROM after the count.
    let bad = SQLParser::new().unwrap().parse("receive 5 q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Count without a queue name.
    let bad = SQLParser::new().unwrap().parse("receive 5 from;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Queue name must not start with a digit.
    let bad = SQLParser::new().unwrap().parse("receive from 1q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // WAIT without a value.
    let bad = SQLParser::new().unwrap().parse("receive from q wait;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Non-numeric WAIT value.
    let bad = SQLParser::new().unwrap().parse("receive from q wait abc;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Negative WAIT value.
    let bad = SQLParser::new().unwrap().parse("receive from q wait -5;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Unexpected token after the queue name.
    let bad = SQLParser::new().unwrap().parse("receive from q hold 5;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // WAIT and FOREVER are mutually exclusive: trailing tokens are rejected.
    let bad = SQLParser::new()
        .unwrap()
        .parse("receive from q wait 5 forever;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    let bad = SQLParser::new()
        .unwrap()
        .parse("receive from q wait forever wait 5;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn ack_success_and_errors() {
    let stmt = parse("ack order_events to 42;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Ack(ack)) = stmt else {
        panic!("expected ack");
    };
    assert_eq!(ack.name(), "order_events");
    assert_eq!(ack.seq(), 42);

    // Case-insensitive keywords and extra whitespace.
    let stmt = parse("ACK  q1  TO  7;").stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::Ack(ack)) = stmt else {
        panic!("expected ack");
    };
    assert_eq!(ack.name(), "q1");
    assert_eq!(ack.seq(), 7);

    // Missing queue name.
    let bad = SQLParser::new().unwrap().parse("ack;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Missing TO clause.
    let bad = SQLParser::new().unwrap().parse("ack q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Wrong keyword in place of TO.
    let bad = SQLParser::new().unwrap().parse("ack q from 1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Missing sequence number.
    let bad = SQLParser::new().unwrap().parse("ack q to;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Non-numeric sequence number.
    let bad = SQLParser::new().unwrap().parse("ack q to abc;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Negative sequence number.
    let bad = SQLParser::new().unwrap().parse("ack q to -1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Trailing garbage after the sequence number.
    let bad = SQLParser::new().unwrap().parse("ack q to 1 extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Queue name must not start with a digit.
    let bad = SQLParser::new().unwrap().parse("ack 1q to 1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn mixed_script_with_queue_statements_parses() {
    let sql = "CREATE QUEUE order_events (id INT, body TEXT);\n\
               INSERT INTO order_events VALUES (1, 'hello');\n\
               RECEIVE 1 FROM order_events WAIT 100;\n\
               ACK order_events TO 1;\n\
               DROP QUEUE order_events;";
    let list = parse(sql);
    assert_eq!(list.stmts().len(), 5);
    assert!(matches!(
        &list.stmts()[0],
        StmtType::Command(StmtCommand::CreateQueue(_))
    ));
    assert!(matches!(
        &list.stmts()[1],
        StmtType::Command(StmtCommand::Insert(_))
    ));
    assert!(matches!(
        &list.stmts()[2],
        StmtType::Command(StmtCommand::Receive(_))
    ));
    assert!(matches!(
        &list.stmts()[3],
        StmtType::Command(StmtCommand::Ack(_))
    ));
    assert!(matches!(
        &list.stmts()[4],
        StmtType::Command(StmtCommand::DropQueue(_))
    ));
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

#[test]
#[cfg_attr(miri, ignore)]
fn split_ignores_semicolons_in_block_comments() {
    use super::split_top_level_statements;

    let sql = "CREATE TABLE t (id INT PRIMARY KEY); /* drop; table; */ INSERT INTO t VALUES (1);";
    let parts = split_top_level_statements(sql);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].contains("CREATE TABLE"));
    assert!(parts[1].contains("INSERT INTO"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn split_ignores_line_comment_markers_in_block_comments() {
    use super::split_top_level_statements;

    // A `--` inside a block comment must not start a line comment, and the
    // `;` after it stays inside the block comment.
    let sql = "SELECT 1; /* -- not a line comment; */ SELECT 2;";
    let parts = split_top_level_statements(sql);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].contains("SELECT 1"));
    assert!(parts[1].contains("SELECT 2"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn split_keeps_block_comment_marker_inside_string_literal() {
    use super::split_top_level_statements;

    let sql = "INSERT INTO t VALUES (1, 'a/*;b'); INSERT INTO t VALUES (2, 'c');";
    let parts = split_top_level_statements(sql);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].contains("'a/*;b'"));
    assert!(parts[1].contains("'c'"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn split_unterminated_block_comment_swallows_rest_of_script() {
    use super::split_top_level_statements;

    let sql = "SELECT 1; /* never closed; SELECT 2; SELECT 3;";
    let parts = split_top_level_statements(sql);
    assert_eq!(parts.len(), 1);
    assert!(parts[0].contains("SELECT 1"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn receive_for_consumer_success_and_errors() {
    // FOR before WAIT.
    let stmt = parse("receive 10 from order_events for cg1 wait 5000;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Receive(receive)) = stmt else {
        panic!("expected receive");
    };
    assert_eq!(receive.name(), "order_events");
    assert_eq!(receive.count(), Some(10));
    assert_eq!(receive.consumer(), Some("cg1"));
    assert_eq!(receive.wait(), Some(ReceiveWait::Timeout(5000)));

    // FOR without WAIT, case-insensitive.
    let stmt = parse("RECEIVE FROM q1 FOR cg2;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Receive(receive)) = stmt else {
        panic!("expected receive");
    };
    assert_eq!(receive.consumer(), Some("cg2"));
    assert_eq!(receive.wait(), None);

    // No FOR: the default competing cursor.
    let stmt = parse("receive from q1;").stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::Receive(receive)) = stmt else {
        panic!("expected receive");
    };
    assert_eq!(receive.consumer(), None);

    // FOR without a consumer name.
    let bad = SQLParser::new().unwrap().parse("receive from q for;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // WAIT before FOR is rejected: FOR must come first.
    let bad = SQLParser::new()
        .unwrap()
        .parse("receive from q wait 5 for cg1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Consumer name must be a valid identifier.
    let bad = SQLParser::new().unwrap().parse("receive from q for 1c;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn ack_for_consumer_success_and_errors() {
    let stmt = parse("ack order_events for cg1 to 42;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::Ack(ack)) = stmt else {
        panic!("expected ack");
    };
    assert_eq!(ack.name(), "order_events");
    assert_eq!(ack.consumer(), Some("cg1"));
    assert_eq!(ack.seq(), 42);

    // No FOR: the default competing cursor.
    let stmt = parse("ack q1 to 7;").stmts().first().unwrap().clone();
    let StmtType::Command(StmtCommand::Ack(ack)) = stmt else {
        panic!("expected ack");
    };
    assert_eq!(ack.consumer(), None);

    // FOR without a consumer name.
    let bad = SQLParser::new().unwrap().parse("ack q for to 1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // FOR but missing TO.
    let bad = SQLParser::new().unwrap().parse("ack q for cg1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Trailing garbage after the sequence number with FOR.
    let bad = SQLParser::new().unwrap().parse("ack q for cg1 to 1 extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn create_consumer_success_and_errors() {
    let stmt = parse("create consumer cg1 on order_events;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateConsumer(create)) = stmt else {
        panic!("expected create consumer");
    };
    assert_eq!(create.consumer(), "cg1");
    assert_eq!(create.queue(), "order_events");

    // Case-insensitive keywords and extra whitespace.
    let stmt = parse("CREATE CONSUMER  cg2  ON  q1;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::CreateConsumer(create)) = stmt else {
        panic!("expected create consumer");
    };
    assert_eq!(create.consumer(), "cg2");
    assert_eq!(create.queue(), "q1");

    // Missing consumer name.
    let bad = SQLParser::new().unwrap().parse("create consumer;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Missing ON clause.
    let bad = SQLParser::new().unwrap().parse("create consumer cg1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Wrong keyword in place of ON.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create consumer cg1 for q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Missing queue name.
    let bad = SQLParser::new().unwrap().parse("create consumer cg1 on;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Trailing garbage.
    let bad = SQLParser::new()
        .unwrap()
        .parse("create consumer cg1 on q extra;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn drop_consumer_success_and_errors() {
    let stmt = parse("drop consumer cg1 on order_events;")
        .stmts()
        .first()
        .unwrap()
        .clone();
    let StmtType::Command(StmtCommand::DropConsumer(drop)) = stmt else {
        panic!("expected drop consumer");
    };
    assert_eq!(drop.consumer(), "cg1");
    assert_eq!(drop.queue(), "order_events");

    // Missing ON clause.
    let bad = SQLParser::new().unwrap().parse("drop consumer cg1;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);

    // Invalid consumer name.
    let bad = SQLParser::new().unwrap().parse("drop consumer 1c on q;");
    assert_eq!(bad.unwrap_err().ec(), ErrorCode::Parse);
}
