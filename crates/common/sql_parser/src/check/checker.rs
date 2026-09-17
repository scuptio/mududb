//! The SQL checker: syntax classification, semantic checks, parameter
//! counting, and `SELECT` result-shape extraction.

use crate::ast::expr_function::FunctionArg;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_name::ExprName;
use crate::ast::expression::ExprType;
use crate::ast::parser::SQLParser;
use crate::ast::select_term::SelectField;
use crate::ast::stmt_select::StmtSelect;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::ast::stmt_update::AssignedValue;
use crate::check::diagnostic::{SqlDiagnostic, SqlSeverity};
use mudu::error::ErrorCode;
use mudu_binding::table::column_def::ColumnDef;
use mudu_binding::table::table_def::TableDef;
use tree_sitter::Node;

/// SQL keywords whose constructs are valid SQL but outside the subset the
/// checker covers (joins, subqueries, grouping, set operations, ...).
///
/// The list is only consulted when a statement failed to parse or convert:
/// a statement that parses cleanly is fully inside the supported subset and
/// never hits this classification.
const BEYOND_SUBSET_KEYWORDS: &[&str] = &[
    "join",
    "inner",
    "left",
    "right",
    "full",
    "outer",
    "cross",
    "natural",
    "using",
    "on",
    "union",
    "intersect",
    "except",
    "group",
    "order",
    "having",
    "limit",
    "offset",
    "fetch",
    "distinct",
    "exists",
    "between",
    "like",
    "in",
    "is",
    "not",
    "case",
    "when",
    "over",
    "recursive",
    "with",
    "returning",
    "conflict",
    "window",
];

/// Check one SQL text against the schema and return all diagnostics.
///
/// The function never fails: every problem is reported as a
/// [`SqlDiagnostic`]. `line`/`column` of each diagnostic are 1-based
/// positions relative to the start of `sql`.
///
/// Classification rules:
///
/// - statements that do not parse and contain a beyond-subset construct
///   (see module docs) yield [`SqlSeverity::Uncovered`]; otherwise a syntax
///   problem yields [`SqlSeverity::Error`];
/// - AST conversion failures with [`ErrorCode::NotImplemented`] yield
///   [`SqlSeverity::Uncovered`]; other conversion failures yield
///   [`SqlSeverity::Error`] unless a beyond-subset keyword is present;
/// - unknown tables/columns yield [`SqlSeverity::Error`].
pub fn check_sql(sql: &str, schema: &[TableDef]) -> Vec<SqlDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_sql::LANGUAGE.into();
    if parser.set_language(&language).is_err() {
        diagnostics.push(SqlDiagnostic::new(
            SqlSeverity::Error,
            "failed to load the SQL grammar".to_string(),
            1,
            1,
        ));
        return diagnostics;
    }
    let opt_tree = parser.parse(sql, None);
    let tree = match opt_tree {
        Some(tree) => tree,
        None => {
            diagnostics.push(SqlDiagnostic::new(
                SqlSeverity::Error,
                "SQL parse error".to_string(),
                1,
                1,
            ));
            return diagnostics;
        }
    };
    let mut error_nodes = Vec::new();
    collect_error_nodes(&tree.root_node(), &mut error_nodes);
    if !error_nodes.is_empty() {
        let keyword = first_beyond_subset_keyword(sql);
        let (severity, message) = match keyword {
            Some(keyword) => (
                SqlSeverity::Uncovered,
                format!(
                    "SQL construct '{keyword}' is beyond the supported subset; \
                     the statement cannot be statically checked"
                ),
            ),
            None => (SqlSeverity::Error, "SQL syntax error".to_string()),
        };
        // Only the first error node is reported: later nodes are usually
        // cascade noise from the same syntax problem.
        if let Some(node) = error_nodes.first() {
            let position = node.start_position();
            diagnostics.push(SqlDiagnostic::new(
                severity,
                message,
                position.row + 1,
                position.column + 1,
            ));
        }
        return diagnostics;
    }

    let sql_parser = match SQLParser::new() {
        Ok(parser) => parser,
        Err(e) => {
            diagnostics.push(SqlDiagnostic::new(
                SqlSeverity::Error,
                format!("failed to initialize the SQL parser: {}", e.message()),
                1,
                1,
            ));
            return diagnostics;
        }
    };
    let stmt_list = match sql_parser.parse(sql) {
        Ok(stmt_list) => stmt_list,
        Err(e) => {
            let (severity, message) = classify_conversion_error(sql, e.message(), e.ec());
            diagnostics.push(SqlDiagnostic::new(severity, message, 1, 1));
            return diagnostics;
        }
    };
    for stmt in stmt_list.stmts() {
        check_stmt(stmt, schema, sql, &mut diagnostics);
    }
    diagnostics
}

/// Count the `?` parameter placeholders in one parsed statement.
///
/// Placeholders are counted in `INSERT` value rows, `UPDATE` assignments
/// (including arithmetic expressions), and `WHERE` predicates.
pub fn count_params(stmt: &StmtType) -> usize {
    let mut count = 0;
    match stmt {
        StmtType::Select(select) => {
            for predicate in select.get_where_predicate() {
                count += count_params_compare(predicate);
            }
        }
        StmtType::Command(command) => match command {
            StmtCommand::Insert(insert) => {
                for row in insert.values_list() {
                    for value in row {
                        if matches!(value, ExprValue::ValuePlaceholder) {
                            count += 1;
                        }
                    }
                }
            }
            StmtCommand::Update(update) => {
                for assignment in update.get_set_values() {
                    match assignment.get_set_value() {
                        AssignedValue::Value(ExprValue::ValuePlaceholder) => count += 1,
                        AssignedValue::Value(_) => {}
                        AssignedValue::Expression(expr) => count += count_params_expr(expr),
                    }
                }
                for predicate in update.get_where_predicate() {
                    count += count_params_compare(predicate);
                }
            }
            StmtCommand::Delete(delete) => {
                for predicate in delete.get_where_predicate() {
                    count += count_params_compare(predicate);
                }
            }
            _ => {}
        },
    }
    count
}

/// Extract the result-column shape of a parsed `SELECT` statement.
///
/// The returned names are in select-list order: plain columns keep their
/// (unqualified) names, `*` expands to the table's columns in DDL order when
/// the table is present in `schema` (and degrades to a literal `"*"`
/// otherwise), and function terms are reported as `"<func>"`.
pub fn select_result_shape(select: &StmtSelect, schema: &[TableDef]) -> Vec<String> {
    let table_def = find_table(schema, select.get_table_reference().name());
    let mut shape = Vec::new();
    for term in select.get_select_term_list() {
        match term.field() {
            SelectField::Column(name) => {
                let raw = name.name();
                if raw.is_empty() {
                    match table_def {
                        Some(def) => shape.extend(
                            def.table_columns()
                                .iter()
                                .map(|column| column.column_name().clone()),
                        ),
                        None => shape.push("*".to_string()),
                    }
                } else {
                    shape.push(unqualified_name(raw));
                }
            }
            SelectField::Function(_) => shape.push("<func>".to_string()),
        }
    }
    shape
}

fn classify_conversion_error(sql: &str, message: &str, code: ErrorCode) -> (SqlSeverity, String) {
    if code == ErrorCode::NotImplemented {
        return (
            SqlSeverity::Uncovered,
            format!("SQL construct beyond the supported subset: {message}"),
        );
    }
    if let Some(keyword) = first_beyond_subset_keyword(sql) {
        return (
            SqlSeverity::Uncovered,
            format!(
                "SQL construct '{keyword}' is beyond the supported subset; \
                 the statement cannot be statically checked"
            ),
        );
    }
    (SqlSeverity::Error, format!("SQL parse error: {message}"))
}

fn check_stmt(
    stmt: &StmtType,
    schema: &[TableDef],
    sql: &str,
    diagnostics: &mut Vec<SqlDiagnostic>,
) {
    match stmt {
        StmtType::Select(select) => {
            let Some(table_def) = check_table(
                select.get_table_reference().name(),
                schema,
                sql,
                diagnostics,
            ) else {
                return;
            };
            for term in select.get_select_term_list() {
                match term.field() {
                    SelectField::Column(name) => {
                        if !name.name().is_empty() {
                            check_column_ref(
                                name,
                                select.get_table_reference().name(),
                                table_def,
                                sql,
                                diagnostics,
                            );
                        }
                    }
                    SelectField::Function(function) => {
                        if let FunctionArg::Column(name) = function.arg() {
                            check_column_ref(
                                name,
                                select.get_table_reference().name(),
                                table_def,
                                sql,
                                diagnostics,
                            );
                        }
                    }
                }
            }
            for predicate in select.get_where_predicate() {
                check_compare(
                    predicate,
                    select.get_table_reference().name(),
                    table_def,
                    sql,
                    diagnostics,
                );
            }
        }
        StmtType::Command(command) => match command {
            StmtCommand::Insert(insert) => {
                let Some(table_def) = check_table(insert.table_name(), schema, sql, diagnostics)
                else {
                    return;
                };
                for column in insert.columns() {
                    check_column_ref_name(column, insert.table_name(), table_def, sql, diagnostics);
                }
            }
            StmtCommand::Update(update) => {
                let Some(table_def) = check_table(
                    update.get_table_reference().name(),
                    schema,
                    sql,
                    diagnostics,
                ) else {
                    return;
                };
                for assignment in update.get_set_values() {
                    check_column_ref_name(
                        assignment.get_column_reference(),
                        update.get_table_reference().name(),
                        table_def,
                        sql,
                        diagnostics,
                    );
                    if let AssignedValue::Expression(expr) = assignment.get_set_value() {
                        check_expr(
                            expr,
                            update.get_table_reference().name(),
                            table_def,
                            sql,
                            diagnostics,
                        );
                    }
                }
                for predicate in update.get_where_predicate() {
                    check_compare(
                        predicate,
                        update.get_table_reference().name(),
                        table_def,
                        sql,
                        diagnostics,
                    );
                }
            }
            StmtCommand::Delete(delete) => {
                let Some(table_def) = check_table(
                    delete.get_table_reference().name(),
                    schema,
                    sql,
                    diagnostics,
                ) else {
                    return;
                };
                for predicate in delete.get_where_predicate() {
                    check_compare(
                        predicate,
                        delete.get_table_reference().name(),
                        table_def,
                        sql,
                        diagnostics,
                    );
                }
            }
            _ => {}
        },
    }
}

fn check_table<'a>(
    table_name: &str,
    schema: &'a [TableDef],
    sql: &str,
    diagnostics: &mut Vec<SqlDiagnostic>,
) -> Option<&'a TableDef> {
    if table_name.is_empty() {
        return None;
    }
    let table_def = find_table(schema, table_name);
    if table_def.is_none() {
        let (line, column) = locate_identifier(sql, table_name).unwrap_or((1, 1));
        diagnostics.push(SqlDiagnostic::new(
            SqlSeverity::Error,
            format!("unknown table '{table_name}': not found in the schema"),
            line,
            column,
        ));
    }
    table_def
}

fn check_compare(
    compare: &crate::ast::expr_compare::ExprCompare,
    table_name: &str,
    table_def: &TableDef,
    sql: &str,
    diagnostics: &mut Vec<SqlDiagnostic>,
) {
    for item in [compare.left(), compare.right()] {
        if let ExprItem::ItemName(name) = item {
            check_column_ref(name, table_name, table_def, sql, diagnostics);
        }
    }
}

fn check_expr(
    expr: &ExprType,
    table_name: &str,
    table_def: &TableDef,
    sql: &str,
    diagnostics: &mut Vec<SqlDiagnostic>,
) {
    match expr {
        ExprType::Value(item) => {
            if let ExprItem::ItemName(name) = item.as_ref() {
                check_column_ref(name, table_name, table_def, sql, diagnostics);
            }
        }
        ExprType::Compare(compare) => {
            check_compare(compare, table_name, table_def, sql, diagnostics);
        }
        ExprType::Logical(logical) => {
            check_expr(logical.left(), table_name, table_def, sql, diagnostics);
            check_expr(logical.right(), table_name, table_def, sql, diagnostics);
        }
        ExprType::Arithmetic(arithmetic) => {
            check_expr(arithmetic.left(), table_name, table_def, sql, diagnostics);
            check_expr(arithmetic.right(), table_name, table_def, sql, diagnostics);
        }
    }
}

fn check_column_ref(
    name: &ExprName,
    table_name: &str,
    table_def: &TableDef,
    sql: &str,
    diagnostics: &mut Vec<SqlDiagnostic>,
) {
    check_column_ref_name(name.name(), table_name, table_def, sql, diagnostics);
}

fn check_column_ref_name(
    raw_name: &str,
    table_name: &str,
    table_def: &TableDef,
    sql: &str,
    diagnostics: &mut Vec<SqlDiagnostic>,
) {
    if raw_name.is_empty() {
        return;
    }
    let (qualifier, column_name) = match raw_name.split_once('.') {
        Some((qualifier, column_name)) => (Some(qualifier), column_name),
        None => (None, raw_name),
    };
    if let Some(qualifier) = qualifier {
        if !qualifier.eq_ignore_ascii_case(table_name) {
            let (line, column) = locate_identifier(sql, raw_name).unwrap_or((1, 1));
            diagnostics.push(SqlDiagnostic::new(
                SqlSeverity::Error,
                format!(
                    "unknown table qualifier '{qualifier}' in '{raw_name}': \
                     the statement targets table '{table_name}'"
                ),
                line,
                column,
            ));
            return;
        }
    }
    if find_column(table_def, column_name).is_none() {
        let (line, column) = locate_identifier(sql, raw_name).unwrap_or((1, 1));
        diagnostics.push(SqlDiagnostic::new(
            SqlSeverity::Error,
            format!(
                "unknown column '{column_name}' in table '{}'",
                table_def.table_name()
            ),
            line,
            column,
        ));
    }
}

fn count_params_compare(compare: &crate::ast::expr_compare::ExprCompare) -> usize {
    [compare.left(), compare.right()]
        .iter()
        .map(|item| count_params_item(item))
        .sum()
}

fn count_params_item(item: &ExprItem) -> usize {
    match item {
        ExprItem::ItemValue(ExprValue::ValuePlaceholder) => 1,
        _ => 0,
    }
}

fn count_params_expr(expr: &ExprType) -> usize {
    match expr {
        ExprType::Value(item) => count_params_item(item),
        ExprType::Compare(compare) => count_params_compare(compare),
        ExprType::Logical(logical) => {
            count_params_expr(logical.left()) + count_params_expr(logical.right())
        }
        ExprType::Arithmetic(arithmetic) => {
            count_params_expr(arithmetic.left()) + count_params_expr(arithmetic.right())
        }
    }
}

/// Find a table by name using ASCII case-insensitive comparison; SQL
/// identifiers are case-insensitive.
pub fn find_table<'a>(schema: &'a [TableDef], table_name: &str) -> Option<&'a TableDef> {
    schema
        .iter()
        .find(|def| def.table_name().eq_ignore_ascii_case(table_name))
}

/// Find a column by name using ASCII case-insensitive comparison.
pub fn find_column<'a>(table_def: &'a TableDef, column_name: &str) -> Option<&'a ColumnDef> {
    table_def
        .table_columns()
        .iter()
        .find(|column| column.column_name().eq_ignore_ascii_case(column_name))
}

fn unqualified_name(raw_name: &str) -> String {
    match raw_name.rsplit_once('.') {
        Some((_, name)) => name.to_string(),
        None => raw_name.to_string(),
    }
}

fn collect_error_nodes<'t>(node: &Node<'t>, error_nodes: &mut Vec<Node<'t>>) {
    if !node.has_error() {
        return;
    }
    if node.kind() == "ERROR" || node.is_missing() {
        error_nodes.push(*node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_error_nodes(&child, error_nodes);
    }
}

/// Return the first beyond-subset keyword in `sql`, skipping single- and
/// double-quoted spans so string literals do not trigger classification.
fn first_beyond_subset_keyword(sql: &str) -> Option<String> {
    let mut stripped = String::with_capacity(sql.len());
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    for ch in sql.chars() {
        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                stripped.push(' ');
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                stripped.push(' ');
            }
            _ if in_single_quote || in_double_quote => stripped.push(' '),
            _ => stripped.push(ch),
        }
    }
    for word in stripped.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if word.is_empty() {
            continue;
        }
        let lowered = word.to_ascii_lowercase();
        if BEYOND_SUBSET_KEYWORDS.contains(&lowered.as_str()) {
            return Some(lowered);
        }
    }
    None
}

/// Locate the first word-boundary, ASCII case-insensitive occurrence of
/// `name` in `sql` and return its 1-based (line, byte-column).
fn locate_identifier(sql: &str, name: &str) -> Option<(usize, usize)> {
    if name.is_empty() || name.len() > sql.len() {
        return None;
    }
    let bytes = sql.as_bytes();
    let mut start = 0;
    while start + name.len() <= sql.len() {
        let candidate = sql.get(start..start + name.len());
        if let Some(candidate) = candidate {
            if candidate.eq_ignore_ascii_case(name) {
                let before_ok = start == 0 || !is_identifier_byte(bytes[start - 1]);
                let after = start + name.len();
                let after_ok = after >= bytes.len() || !is_identifier_byte(bytes[after]);
                if before_ok && after_ok {
                    return Some(offset_to_line_column(sql, start));
                }
            }
        }
        start += 1;
    }
    None
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn offset_to_line_column(sql: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut line_start = 0;
    for (index, byte) in sql.bytes().enumerate() {
        if index >= offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    (line, offset - line_start + 1)
}
