//! Placeholder-to-column mapping for the supported DML subset, and the
//! re-exported parameter/column type compatibility table (defined in
//! `mudu_binding::universal::uni_type_compat` so the syscall frame
//! decoders, the host checker, and this crate share one table).
//!
//! The mapping aligns with [`placeholder_spans`] order: entry `i` is the
//! target column of the `i`-th `?` placeholder, or `None` when the
//! placeholder has no statically known target (a `?` in a select list, a
//! `? = ?` predicate, an unknown table/column, or SQL beyond the supported
//! subset). Unmappable placeholders are skipped, never an error — that is
//! what keeps this usable as a best-effort pre-flight check.

use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expression::ExprType;
use crate::ast::parser::SQLParser;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::ast::stmt_update::{AssignedValue, Assignment};
use crate::check::checker::{find_column, find_table};
use crate::check::placeholder::placeholder_spans;
use mudu::common::result::RS;
use mudu_binding::table::column_def::ColumnDef;
use mudu_binding::table::table_def::TableDef;

pub use mudu_binding::universal::uni_type_compat::{
    param_type_compatible, param_type_tag_of_value, uni_data_type_name, ParamTypeTag,
};

/// Map every `?` placeholder in `sql` (in [`placeholder_spans`] order) to
/// its target column in `schema`.
///
/// Returns one entry per placeholder; `None` marks placeholders without a
/// statically known target (never an error). SQL that does not parse maps
/// to all-`None`.
pub fn param_target_columns<'a>(
    sql: &str,
    schema: &'a [TableDef],
) -> RS<Vec<Option<&'a ColumnDef>>> {
    let span_count = placeholder_spans(sql)?.len();
    let parser = SQLParser::new()?;
    let stmt_list = match parser.parse(sql) {
        Ok(stmt_list) => stmt_list,
        Err(_) => return Ok(vec![None; span_count]),
    };
    let mut columns: Vec<Option<&ColumnDef>> = Vec::with_capacity(span_count);
    for stmt in stmt_list.stmts() {
        map_stmt_columns(stmt, schema, &mut columns);
    }
    // Align defensively with the span scan: pad with None, never overfill.
    columns.resize(span_count, None);
    columns.truncate(span_count);
    Ok(columns)
}

fn map_stmt_columns<'a>(
    stmt: &StmtType,
    schema: &'a [TableDef],
    out: &mut Vec<Option<&'a ColumnDef>>,
) {
    match stmt {
        StmtType::Select(select) => {
            let table_def = find_table(schema, select.get_table_reference().name());
            for predicate in select.get_where_predicate() {
                map_compare_columns(predicate, table_def, out);
            }
        }
        StmtType::Command(command) => match command {
            StmtCommand::Insert(insert) => {
                let table_def = find_table(schema, insert.table_name());
                for row in insert.values_list() {
                    for (position, value) in row.iter().enumerate() {
                        if matches!(value, ExprValue::ValuePlaceholder) {
                            let column = insert
                                .columns()
                                .get(position)
                                .and_then(|name| column_of(table_def, name));
                            out.push(column);
                        }
                    }
                }
            }
            StmtCommand::Update(update) => {
                let table_def = find_table(schema, update.get_table_reference().name());
                for assignment in update.get_set_values() {
                    map_assignment_columns(assignment, table_def, out);
                }
                for predicate in update.get_where_predicate() {
                    map_compare_columns(predicate, table_def, out);
                }
            }
            StmtCommand::Delete(delete) => {
                let table_def = find_table(schema, delete.get_table_reference().name());
                for predicate in delete.get_where_predicate() {
                    map_compare_columns(predicate, table_def, out);
                }
            }
            _ => {}
        },
    }
}

fn map_assignment_columns<'a>(
    assignment: &Assignment,
    table_def: Option<&'a TableDef>,
    out: &mut Vec<Option<&'a ColumnDef>>,
) {
    let target = column_of(table_def, assignment.get_column_reference());
    match assignment.get_set_value() {
        AssignedValue::Value(ExprValue::ValuePlaceholder) => out.push(target),
        AssignedValue::Value(_) => {}
        AssignedValue::Expression(expr) => map_expr_columns(expr, target, out),
    }
}

fn map_expr_columns<'a>(
    expr: &ExprType,
    column: Option<&'a ColumnDef>,
    out: &mut Vec<Option<&'a ColumnDef>>,
) {
    match expr {
        ExprType::Value(item) => {
            if matches!(
                item.as_ref(),
                ExprItem::ItemValue(ExprValue::ValuePlaceholder)
            ) {
                out.push(column);
            }
        }
        ExprType::Compare(compare) => {
            map_expr_columns_side(compare.left(), column, out);
            map_expr_columns_side(compare.right(), column, out);
        }
        ExprType::Logical(logical) => {
            map_expr_columns(logical.left(), column, out);
            map_expr_columns(logical.right(), column, out);
        }
        ExprType::Arithmetic(arithmetic) => {
            map_expr_columns(arithmetic.left(), column, out);
            map_expr_columns(arithmetic.right(), column, out);
        }
    }
}

fn map_expr_columns_side<'a>(
    item: &ExprItem,
    column: Option<&'a ColumnDef>,
    out: &mut Vec<Option<&'a ColumnDef>>,
) {
    if matches!(item, ExprItem::ItemValue(ExprValue::ValuePlaceholder)) {
        out.push(column);
    }
}

fn map_compare_columns<'a>(
    compare: &ExprCompare,
    table_def: Option<&'a TableDef>,
    out: &mut Vec<Option<&'a ColumnDef>>,
) {
    // A placeholder takes the column of the *other* side of the comparison;
    // `? = ?` and column-less sides map to None.
    let right_column = item_column(compare.right(), table_def);
    map_expr_columns_side(compare.left(), right_column, out);
    let left_column = item_column(compare.left(), table_def);
    map_expr_columns_side(compare.right(), left_column, out);
}

fn item_column<'a>(item: &ExprItem, table_def: Option<&'a TableDef>) -> Option<&'a ColumnDef> {
    match item {
        ExprItem::ItemName(name) => column_of(table_def, name.name()),
        _ => None,
    }
}

fn column_of<'a>(table_def: Option<&'a TableDef>, name: &str) -> Option<&'a ColumnDef> {
    let unqualified = match name.rsplit_once('.') {
        Some((_, column)) => column,
        None => name,
    };
    table_def.and_then(|def| find_column(def, unqualified))
}

#[cfg(test)]
#[path = "param_type_test.rs"]
mod param_type_test;
