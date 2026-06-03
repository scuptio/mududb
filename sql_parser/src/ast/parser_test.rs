#[cfg(test)]
mod tests {
    use crate::ast::expr_item::{ExprItem, ExprValue};
    use crate::ast::expr_operator::{Arithmetic, ValueCompare};
    use crate::ast::expression::ExprType;
    use crate::ast::parser::SQLParser;
    use crate::ast::stmt_create_table::StmtCreateTable;
    use crate::ast::stmt_type::{StmtCommand, StmtType};
    use crate::ast::stmt_update::AssignedValue;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_dat_type::UniDatType;
    use mudu_binding::universal::uni_dat_value::UniDatValue;
    use mudu_binding::universal::uni_scalar::UniScalar;
    use mudu_binding::universal::uni_scalar_value::UniScalarValue;
    use project_root::get_project_root;
    use std::fs;
    use std::path::Path;

    fn parse_sql(sql: &str) -> RS<Vec<StmtType>> {
        let parser = SQLParser::new();
        Ok(parser.parse(sql)?.stmts().clone())
    }

    fn parse_create_table(sql: &str) -> RS<StmtCreateTable> {
        let stmts = parse_sql(sql)?;
        let stmt = stmts.first().ok_or_else(|| {
            mudu::m_error!(mudu::error::ec::EC::ParseErr, "expected one statement")
        })?;
        match stmt {
            StmtType::Command(StmtCommand::CreateTable(stmt)) => Ok(stmt.clone()),
            _ => Err(mudu::m_error!(
                mudu::error::ec::EC::ParseErr,
                "expected create table statement"
            )),
        }
    }

    fn parse_file<P: AsRef<Path>>(path: P) -> RS<Vec<StmtType>> {
        let sql = fs::read_to_string(path).unwrap();
        parse_sql(&sql)
    }

    #[test]
    fn parse_select_where_extracts_compare_predicates() {
        let stmts =
            parse_sql("select id, name from users where id = 1 AND name = 'alice';").unwrap();

        let StmtType::Select(stmt) = &stmts[0] else {
            panic!("expected select");
        };
        assert_eq!(stmt.get_table_reference(), "users");
        assert_eq!(stmt.get_select_term_list().len(), 2);
        assert_eq!(stmt.get_where_predicate().len(), 2);
        assert!(matches!(
            stmt.get_where_predicate()[0].op(),
            ValueCompare::EQ
        ));
        assert!(matches!(
            stmt.get_where_predicate()[1].op(),
            ValueCompare::EQ
        ));
    }

    #[test]
    fn parse_select_with_placeholder_keeps_value_placeholder() {
        let stmts = parse_sql("select id from users where id = ?;").unwrap();

        let StmtType::Select(stmt) = &stmts[0] else {
            panic!("expected select");
        };
        let predicate = &stmt.get_where_predicate()[0];
        match predicate.right() {
            ExprItem::ItemValue(ExprValue::ValuePlaceholder) => {}
            other => panic!("expected placeholder, got {other:?}"),
        }
    }

    #[test]
    fn parse_select_reverts_literal_field_comparison_shape() {
        let stmts = parse_sql("select id from users where 7 > id;").unwrap();

        let StmtType::Select(stmt) = &stmts[0] else {
            panic!("expected select");
        };
        let predicate = stmt.get_where_predicate()[0]
            .expr_field_op_literal()
            .expect("expected field-literal pair");
        assert_eq!(predicate.0.name(), "id");
        assert_eq!(
            predicate.1.dat_type().dat_type().dat_type_id(),
            mudu_type::dat_type_id::DatTypeID::I64
        );
        assert!(matches!(predicate.2, ValueCompare::LE));
    }

    #[test]
    fn parse_decimal_literal_preserves_numeric_type() {
        let stmts = parse_sql("select id from users where amount = 12.34;").unwrap();

        let StmtType::Select(stmt) = &stmts[0] else {
            panic!("expected select");
        };
        let predicate = &stmt.get_where_predicate()[0];
        match predicate.right() {
            ExprItem::ItemValue(ExprValue::ValueLiteral(literal)) => {
                assert_eq!(
                    literal.dat_type().dat_type().dat_type_id(),
                    mudu_type::dat_type_id::DatTypeID::Numeric
                );
                assert_eq!(
                    literal
                        .dat_type()
                        .dat_internal()
                        .expect_numeric()
                        .to_plain_string(),
                    "12.34"
                );
            }
            other => panic!("expected numeric literal on right side, got {other:?}"),
        }
    }

    #[test]
    fn parse_decimal_literal_preserves_trailing_fractional_zeros() {
        let stmts = parse_sql("select id from users where amount = 12.3400;").unwrap();

        let StmtType::Select(stmt) = &stmts[0] else {
            panic!("expected select");
        };
        let predicate = &stmt.get_where_predicate()[0];
        match predicate.right() {
            ExprItem::ItemValue(ExprValue::ValueLiteral(literal)) => {
                assert_eq!(
                    literal
                        .dat_type()
                        .dat_internal()
                        .expect_numeric()
                        .to_plain_string(),
                    "12.3400"
                );
            }
            other => panic!("expected numeric literal on right side, got {other:?}"),
        }
    }

    #[test]
    fn parse_insert_without_column_list() {
        let stmts = parse_sql("insert into users values (1, 'alice');").unwrap();

        let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
            panic!("expected insert");
        };
        assert_eq!(stmt.table_name(), "users");
        assert!(stmt.columns().is_empty());
        assert_eq!(stmt.values_list().len(), 1);
        assert_eq!(stmt.values_list()[0].len(), 2);
    }

    #[test]
    fn parse_multi_row_insert_keeps_each_row() {
        let stmts =
            parse_sql("insert into users (id, name) values (1, 'alice'), (2, 'bob');").unwrap();

        let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
            panic!("expected insert");
        };
        assert_eq!(stmt.columns(), &vec!["id".to_string(), "name".to_string()]);
        assert_eq!(stmt.values_list().len(), 2);
    }

    #[test]
    fn parse_insert_null_literal() {
        let stmts = parse_sql("insert into users (id, name) values (1, null);").unwrap();

        let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
            panic!("expected insert");
        };
        assert!(matches!(
            stmt.values_list()[0][1],
            ExprValue::ValueLiteral(crate::ast::expr_literal::ExprLiteral::Null)
        ));
    }

    #[test]
    fn parse_multiple_statements_with_trailing_semicolons() {
        let stmts =
            parse_sql("insert into users values (1); delete from users where id = 1;").unwrap();
        assert_eq!(stmts.len(), 2);
        assert!(matches!(
            stmts[0],
            StmtType::Command(StmtCommand::Insert(_))
        ));
        assert!(matches!(
            stmts[1],
            StmtType::Command(StmtCommand::Delete(_))
        ));
    }

    #[test]
    fn parse_update_distinguishes_value_and_expression_assignments() {
        let stmts =
            parse_sql("update users set count = 1, total = count + 1 where id = 1;").unwrap();

        let StmtType::Command(StmtCommand::Update(stmt)) = &stmts[0] else {
            panic!("expected update");
        };
        assert_eq!(stmt.get_set_values().len(), 2);
        assert!(matches!(
            stmt.get_set_values()[0].get_set_value(),
            AssignedValue::Value(_)
        ));
        match stmt.get_set_values()[1].get_set_value() {
            AssignedValue::Expression(ExprType::Arithmetic(expr)) => {
                assert!(matches!(expr.op(), Arithmetic::PLUS));
            }
            other => panic!("expected arithmetic assignment, got {other:?}"),
        }
        assert_eq!(stmt.get_where_predicate().len(), 1);
    }

    #[test]
    fn parse_update_keeps_arithmetic_precedence_shape() {
        let stmts = parse_sql("update users set total = count + 1 * 2 where id = 1;").unwrap();

        let StmtType::Command(StmtCommand::Update(stmt)) = &stmts[0] else {
            panic!("expected update");
        };
        match stmt.get_set_values()[0].get_set_value() {
            AssignedValue::Expression(ExprType::Arithmetic(expr)) => {
                assert!(matches!(expr.op(), Arithmetic::PLUS));
                match expr.right() {
                    ExprType::Arithmetic(nested) => {
                        assert!(matches!(nested.op(), Arithmetic::MULTIPLE));
                    }
                    other => panic!("expected nested multiply, got {other:?}"),
                }
            }
            other => panic!("expected arithmetic assignment, got {other:?}"),
        }
    }

    #[test]
    fn parse_delete_with_and_predicates() {
        let stmts = parse_sql("delete from users where id = 1 AND name = 'alice';").unwrap();

        let StmtType::Command(StmtCommand::Delete(stmt)) = &stmts[0] else {
            panic!("expected delete");
        };
        assert_eq!(stmt.get_table_reference(), "users");
        assert_eq!(stmt.get_where_predicate().len(), 2);
    }

    #[test]
    fn parse_drop_table_if_exists() {
        let stmts = parse_sql("drop table if exists users;").unwrap();

        let StmtType::Command(StmtCommand::DropTable(stmt)) = &stmts[0] else {
            panic!("expected drop table");
        };
        assert_eq!(stmt.table_name(), "users");
        assert!(stmt.drop_if_exists());
    }

    #[test]
    fn parse_copy_from_statement() {
        let stmts = parse_sql("copy users from 'users.csv';").unwrap();

        let StmtType::Command(StmtCommand::CopyFrom(stmt)) = &stmts[0] else {
            panic!("expected copy from");
        };
        assert_eq!(stmt.copy_to_table_name(), "users");
        assert_eq!(stmt.copy_from_file_path(), "'users.csv'");
        assert!(stmt.table_columns().is_empty());
    }

    #[test]
    fn parse_invalid_sql_reports_syntax_context() {
        let err = parse_sql("select from users where").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("Syntax error"));
        assert!(text.contains("select from users where"));
        assert!(text.contains("position"));
    }

    #[test]
    fn parse_update_without_where_returns_error() {
        let err = parse_sql("update users set id = 1;").unwrap_err();
        assert!(
            err.to_string()
                .contains("no where clause in update statement")
        );
    }

    #[test]
    fn parse_delete_without_where_returns_error() {
        let err = parse_sql("delete from users;").unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("Syntax error") || text.contains("no where clause in delete statement")
        );
    }

    #[test]
    fn parse_insert_without_values_returns_error() {
        let err = parse_sql("insert into users (id, name);").unwrap_err();
        assert!(err.to_string().contains("Syntax error"));
    }

    #[test]
    fn parse_create_table_with_unsupported_type_returns_error() {
        let err = parse_sql("create table users (id boolean primary key);").unwrap_err();
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[test]
    fn parse_copy_to_statement() {
        let stmts = parse_sql("copy users to 'users.csv';").unwrap();

        let StmtType::Command(StmtCommand::CopyTo(stmt)) = &stmts[0] else {
            panic!("expected copy to");
        };
        assert_eq!(stmt.copy_from_table_name(), "users");
        assert_eq!(stmt.copy_to_file_path(), "'users.csv'");
        assert!(stmt.table_columns().is_empty());
    }

    #[test]
    fn test_create_table() {
        let sql = "
    CREATE TABLE Persons (
        PersonID int PRIMARY KEY,
        LastName char(255),
        FirstName char(255),
        Address char(255),
        City char(255)
    );";
        let r = parse_sql(sql);
        assert!(r.is_ok());

        let sql = "
    CREATE TABLE CUSTOMERS(
           ID1          INT,
           ID2          INT,
           NAME        CHAR (20),
           AGE         INT,
           ADDRESS     CHAR (25),
           SALARY      INT,
           PRIMARY KEY (ID1, ID2)
    );";
        let r = parse_sql(sql);
        assert!(r.is_ok());
    }

    #[test]
    fn parse_create_table_tracks_nullable_constraints() {
        let stmt = parse_create_table(
            "
            CREATE TABLE users (
                id INT PRIMARY KEY,
                name CHAR(32) NOT NULL,
                nickname CHAR(32)
            );
            ",
        )
        .unwrap();

        let columns = stmt.column_def();
        assert!(!columns[0].nullable());
        assert!(!columns[1].nullable());
        assert!(columns[2].nullable());
    }

    #[test]
    fn parse_create_table_numeric_type_preserves_precision_and_scale() {
        let stmt = parse_create_table(
            "
            CREATE TABLE ledger (
                id BIGINT PRIMARY KEY,
                amount NUMERIC(18, 2)
            );
            ",
        )
        .unwrap();

        let amount = stmt
            .non_primary_columns()
            .into_iter()
            .find(|column| column.column_name() == "amount")
            .expect("amount column");
        assert!(matches!(
            amount.data_type(),
            UniDatType::Scalar(UniScalar::Numeric)
        ));
        let params = amount
            .data_type_param()
            .as_ref()
            .expect("numeric params should exist");
        assert_eq!(params.len(), 2);
        assert!(matches!(
            params[0],
            UniDatValue::Scalar(UniScalarValue::I64(18))
        ));
        assert!(matches!(
            params[1],
            UniDatValue::Scalar(UniScalarValue::I64(2))
        ));
    }

    #[test]
    fn parse_create_table_decimal_alias_maps_to_numeric_scalar() {
        let stmt = parse_create_table(
            "
            CREATE TABLE ledger (
                id BIGINT PRIMARY KEY,
                amount DECIMAL(9, 4)
            );
            ",
        )
        .unwrap();

        let amount = stmt
            .non_primary_columns()
            .into_iter()
            .find(|column| column.column_name() == "amount")
            .expect("amount column");
        assert!(matches!(
            amount.data_type(),
            UniDatType::Scalar(UniScalar::Numeric)
        ));
        let params = amount.data_type_param().as_ref().expect("decimal params");
        assert!(matches!(
            params[0],
            UniDatValue::Scalar(UniScalarValue::I64(9))
        ));
        assert!(matches!(
            params[1],
            UniDatValue::Scalar(UniScalarValue::I64(4))
        ));
    }

    #[test]
    fn parse_create_table_numeric_without_params_keeps_numeric_type_without_param_payload() {
        let stmt = parse_create_table(
            "
            CREATE TABLE ledger (
                id BIGINT PRIMARY KEY,
                amount NUMERIC
            );
            ",
        )
        .unwrap();

        let amount = stmt
            .non_primary_columns()
            .into_iter()
            .find(|column| column.column_name() == "amount")
            .expect("amount column");
        assert!(matches!(
            amount.data_type(),
            UniDatType::Scalar(UniScalar::Numeric)
        ));
        assert!(amount.data_type_param().is_none());
    }

    #[test]
    fn parse_create_table_temporal_types_preserve_scalar_kinds_and_precision() {
        let stmt = parse_create_table(
            "
            CREATE TABLE events (
                id BIGINT PRIMARY KEY,
                event_date DATE,
                event_time TIME(3),
                created_at TIMESTAMP(4),
                published_at TIMESTAMPTZ(2)
            );
            ",
        )
        .unwrap();

        let columns = stmt.non_primary_columns();
        assert!(matches!(
            columns[0].data_type(),
            UniDatType::Scalar(UniScalar::Date)
        ));
        assert!(columns[0].data_type_param().is_none());

        assert!(matches!(
            columns[1].data_type(),
            UniDatType::Scalar(UniScalar::Time)
        ));
        let time_params = columns[1]
            .data_type_param()
            .as_ref()
            .expect("time params should exist");
        assert_eq!(time_params.len(), 1);
        assert!(matches!(
            time_params[0],
            UniDatValue::Scalar(UniScalarValue::I64(3))
        ));

        assert!(matches!(
            columns[2].data_type(),
            UniDatType::Scalar(UniScalar::Timestamp)
        ));
        let timestamp_params = columns[2]
            .data_type_param()
            .as_ref()
            .expect("timestamp params should exist");
        assert_eq!(timestamp_params.len(), 1);
        assert!(matches!(
            timestamp_params[0],
            UniDatValue::Scalar(UniScalarValue::I64(4))
        ));

        assert!(matches!(
            columns[3].data_type(),
            UniDatType::Scalar(UniScalar::TimestampTz)
        ));
        let timestamptz_params = columns[3]
            .data_type_param()
            .as_ref()
            .expect("timestamptz params should exist");
        assert_eq!(timestamptz_params.len(), 1);
        assert!(matches!(
            timestamptz_params[0],
            UniDatValue::Scalar(UniScalarValue::I64(2))
        ));
    }

    #[test]
    fn parse_create_table_temporal_types_without_precision_keep_no_param_payload() {
        let stmt = parse_create_table(
            "
            CREATE TABLE audit_log (
                id BIGINT PRIMARY KEY,
                event_time TIME,
                created_at TIMESTAMP,
                published_at TIMESTAMPTZ
            );
            ",
        )
        .unwrap();

        let columns = stmt.non_primary_columns();
        assert!(columns[0].data_type_param().is_none());
        assert!(columns[1].data_type_param().is_none());
        assert!(columns[2].data_type_param().is_none());
    }

    #[test]
    fn parse_create_table_hugeint_maps_to_i128() {
        let stmt = parse_create_table(
            "
            CREATE TABLE ledger (
                id BIGINT PRIMARY KEY,
                amount HUGEINT
            );
            ",
        )
        .unwrap();

        let amount = stmt
            .non_primary_columns()
            .into_iter()
            .find(|column| column.column_name() == "amount")
            .expect("amount column");
        assert!(matches!(
            amount.data_type(),
            UniDatType::Scalar(UniScalar::I128)
        ));
        assert!(amount.data_type_param().is_none());
    }

    #[test]
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
}
