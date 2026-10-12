#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;

#[test]
#[cfg_attr(miri, ignore)]
fn parse_select_where_extracts_compare_predicates() {
    let stmts = parse_sql("select id, name from users where id = 1 AND name = 'alice';").unwrap();

    let StmtType::Select(stmt) = &stmts[0] else {
        panic!("expected select");
    };
    assert_eq!(stmt.get_table_reference().name(), "users");
    assert!(stmt.get_table_reference().schema().is_none());
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
#[cfg_attr(miri, ignore)]
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
#[cfg_attr(miri, ignore)]
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
        predicate.1.data_type().unwrap().data_type().type_family(),
        mudu_type::type_family::TypeFamily::I64
    );
    assert!(matches!(predicate.2, ValueCompare::LE));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_decimal_literal_preserves_numeric_type() {
    let stmts = parse_sql("select id from users where amount = 12.34;").unwrap();

    let StmtType::Select(stmt) = &stmts[0] else {
        panic!("expected select");
    };
    let predicate = &stmt.get_where_predicate()[0];
    match predicate.right() {
        ExprItem::ItemValue(ExprValue::ValueLiteral(literal)) => {
            assert_eq!(
                literal.data_type().unwrap().data_type().type_family(),
                mudu_type::type_family::TypeFamily::Numeric
            );
            assert_eq!(
                literal
                    .data_type()
                    .unwrap()
                    .data_internal()
                    .expect_numeric()
                    .to_plain_string(),
                "12.34"
            );
        }
        other => panic!("expected numeric literal on right side, got {other:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
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
                    .data_type()
                    .unwrap()
                    .data_internal()
                    .expect_numeric()
                    .to_plain_string(),
                "12.3400"
            );
        }
        other => panic!("expected numeric literal on right side, got {other:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
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
#[cfg_attr(miri, ignore)]
fn parse_multi_row_insert_keeps_each_row() {
    let stmts = parse_sql("insert into users (id, name) values (1, 'alice'), (2, 'bob');").unwrap();

    let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
        panic!("expected insert");
    };
    assert_eq!(stmt.columns(), &vec!["id".to_string(), "name".to_string()]);
    assert_eq!(stmt.values_list().len(), 2);
}

#[test]
#[cfg_attr(miri, ignore)]
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
#[cfg_attr(miri, ignore)]
fn parse_multiple_statements_with_trailing_semicolons() {
    let stmts = parse_sql("insert into users values (1); delete from users where id = 1;").unwrap();
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
#[cfg_attr(miri, ignore)]
fn parse_update_distinguishes_value_and_expression_assignments() {
    let stmts = parse_sql("update users set count = 1, total = count + 1 where id = 1;").unwrap();

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
#[cfg_attr(miri, ignore)]
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
#[cfg_attr(miri, ignore)]
fn parse_delete_with_and_predicates() {
    let stmts = parse_sql("delete from users where id = 1 AND name = 'alice';").unwrap();

    let StmtType::Command(StmtCommand::Delete(stmt)) = &stmts[0] else {
        panic!("expected delete");
    };
    assert_eq!(stmt.get_table_reference().name(), "users");
    assert!(stmt.get_table_reference().schema().is_none());
    assert_eq!(stmt.get_where_predicate().len(), 2);
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_qualified_object_reference_across_statement_kinds() {
    let stmts = parse_sql("select id from shop.users where id = 1;").unwrap();
    let StmtType::Select(stmt) = &stmts[0] else {
        panic!("expected select");
    };
    assert_eq!(stmt.get_table_reference().name(), "users");
    assert_eq!(
        stmt.get_table_reference().schema(),
        Some(&"shop".to_string())
    );

    let stmts = parse_sql("insert into shop.users (id) values (1);").unwrap();
    let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
        panic!("expected insert");
    };
    assert_eq!(stmt.table_reference().name(), "users");
    assert_eq!(stmt.table_reference().schema(), Some(&"shop".to_string()));
    assert_eq!(stmt.table_name(), "users");

    let stmts = parse_sql("update shop.users set id = 2 where id = 1;").unwrap();
    let StmtType::Command(StmtCommand::Update(stmt)) = &stmts[0] else {
        panic!("expected update");
    };
    assert_eq!(stmt.get_table_reference().name(), "users");
    assert_eq!(
        stmt.get_table_reference().schema(),
        Some(&"shop".to_string())
    );

    let stmts = parse_sql("delete from shop.users where id = 1;").unwrap();
    let StmtType::Command(StmtCommand::Delete(stmt)) = &stmts[0] else {
        panic!("expected delete");
    };
    assert_eq!(stmt.get_table_reference().name(), "users");
    assert_eq!(
        stmt.get_table_reference().schema(),
        Some(&"shop".to_string())
    );

    let stmts = parse_sql("drop table shop.users;").unwrap();
    let StmtType::Command(StmtCommand::DropTable(stmt)) = &stmts[0] else {
        panic!("expected drop table");
    };
    assert_eq!(stmt.table_reference().name(), "users");
    assert_eq!(stmt.table_reference().schema(), Some(&"shop".to_string()));
    assert_eq!(stmt.table_name(), "users");

    let stmts = parse_sql("copy shop.users from 'users.csv';").unwrap();
    let StmtType::Command(StmtCommand::CopyFrom(stmt)) = &stmts[0] else {
        panic!("expected copy from");
    };
    assert_eq!(stmt.table_reference().name(), "users");
    assert_eq!(stmt.table_reference().schema(), Some(&"shop".to_string()));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_unqualified_object_reference_has_no_schema() {
    let stmts = parse_sql("insert into users (id) values (1);").unwrap();
    let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
        panic!("expected insert");
    };
    assert!(stmt.table_reference().schema().is_none());

    let stmts = parse_sql("drop table if exists users;").unwrap();
    let StmtType::Command(StmtCommand::DropTable(stmt)) = &stmts[0] else {
        panic!("expected drop table");
    };
    assert!(stmt.table_reference().schema().is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_drop_table_if_exists() {
    let stmts = parse_sql("drop table if exists users;").unwrap();

    let StmtType::Command(StmtCommand::DropTable(stmt)) = &stmts[0] else {
        panic!("expected drop table");
    };
    assert_eq!(stmt.table_name(), "users");
    assert!(stmt.drop_if_exists());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_copy_from_statement() {
    let stmts = parse_sql("copy users from 'users.csv';").unwrap();

    let StmtType::Command(StmtCommand::CopyFrom(stmt)) = &stmts[0] else {
        panic!("expected copy from");
    };
    assert_eq!(stmt.copy_to_table_name(), "users");
    assert_eq!(stmt.copy_from_file_path(), "users.csv");
    assert!(stmt.table_columns().is_empty());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_invalid_sql_reports_syntax_context() {
    let err = parse_sql("select from users where").unwrap_err();
    let text = err.to_string();
    assert!(text.contains("Syntax error"));
    assert!(text.contains("select from users where"));
    assert!(text.contains("position"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_update_without_where_returns_error() {
    let err = parse_sql("update users set id = 1;").unwrap_err();
    assert!(err
        .to_string()
        .contains("no where clause in update statement"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_delete_without_where_returns_error() {
    let err = parse_sql("delete from users;").unwrap_err();
    let text = err.to_string();
    assert!(text.contains("Syntax error") || text.contains("no where clause in delete statement"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_insert_without_values_returns_error() {
    let err = parse_sql("insert into users (id, name);").unwrap_err();
    assert!(err.to_string().contains("Syntax error"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_with_unsupported_type_returns_error() {
    let err = parse_sql("create table users (id boolean primary key);").unwrap_err();
    assert!(err.to_string().contains("not yet implemented"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_copy_to_statement() {
    let stmts = parse_sql("copy users to 'users.csv';").unwrap();

    let StmtType::Command(StmtCommand::CopyTo(stmt)) = &stmts[0] else {
        panic!("expected copy to");
    };
    assert_eq!(stmt.copy_from_table_name(), "users");
    assert_eq!(stmt.copy_to_file_path(), "users.csv");
    assert!(stmt.table_columns().is_empty());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_insert_multibyte_string_literal_preserves_value() {
    let stmts = parse_sql("insert into t values ('汉字');").unwrap();

    let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
        panic!("expected insert");
    };
    match &stmt.values_list()[0][0] {
        ExprValue::ValueLiteral(crate::ast::expr_literal::ExprLiteral::DatumLiteral(typed)) => {
            assert_eq!(
                typed.data_internal().as_string().expect("string value"),
                "汉字"
            );
        }
        other => panic!("expected string literal, got {other:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_insert_string_literal_strips_quotes() {
    let stmts = parse_sql("insert into users (id, name) values (1, 'alice');").unwrap();

    let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
        panic!("expected insert");
    };
    match &stmt.values_list()[0][1] {
        ExprValue::ValueLiteral(crate::ast::expr_literal::ExprLiteral::DatumLiteral(typed)) => {
            assert_eq!(
                typed.data_internal().as_string().expect("string value"),
                "alice"
            );
        }
        other => panic!("expected string literal, got {other:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_insert_empty_string_literal() {
    let stmts = parse_sql("insert into users (id, name) values (1, '');").unwrap();

    let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
        panic!("expected insert");
    };
    match &stmt.values_list()[0][1] {
        ExprValue::ValueLiteral(crate::ast::expr_literal::ExprLiteral::DatumLiteral(typed)) => {
            assert_eq!(typed.data_internal().as_string().expect("string value"), "");
        }
        other => panic!("expected string literal, got {other:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_insert_escaped_quote_string_literal() {
    let stmts = parse_sql("insert into users (id, name) values (1, 'it''s');").unwrap();

    let StmtType::Command(StmtCommand::Insert(stmt)) = &stmts[0] else {
        panic!("expected insert");
    };
    match &stmt.values_list()[0][1] {
        ExprValue::ValueLiteral(crate::ast::expr_literal::ExprLiteral::DatumLiteral(typed)) => {
            assert_eq!(
                typed.data_internal().as_string().expect("string value"),
                "it's"
            );
        }
        other => panic!("expected string literal, got {other:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_where_string_literal_strips_quotes() {
    let stmts = parse_sql("select id from users where name = 'alice';").unwrap();

    let StmtType::Select(stmt) = &stmts[0] else {
        panic!("expected select");
    };
    let predicate = &stmt.get_where_predicate()[0];
    match predicate.right() {
        ExprItem::ItemValue(ExprValue::ValueLiteral(
            crate::ast::expr_literal::ExprLiteral::DatumLiteral(typed),
        )) => {
            assert_eq!(
                typed.data_internal().as_string().expect("string value"),
                "alice"
            );
        }
        other => panic!("expected string literal on right side, got {other:?}"),
    }
}
