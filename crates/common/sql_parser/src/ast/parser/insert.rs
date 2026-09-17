//! INSERT statement parser.

use super::context::ParseContext;
use super::error::ts_node_context_string;
use super::SQLParser;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expression::ExprType;
use crate::ast::stmt_insert::StmtInsert;
use crate::ts_const::ts_field_name;
use mudu::common::result::RS;
use mudu::common::result_of::{rs_of_opt, rs_option};
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use tree_sitter::Node;

impl SQLParser {
    /// Parse an INSERT statement node into a [`StmtInsert`].
    pub(crate) fn visit_insert_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtInsert> {
        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let c = rs_option(opt, "no object reference in insert statement")?;
        let table_name = self.visit_object_reference(context, c)?;

        let opt = node.child_by_field_name(ts_field_name::INSERT_VALUES);
        let c = rs_option(opt, "no insert values clause in insert statement")?;
        let (columns, values) = self.visit_insert_values(context, c)?;
        let stmt = StmtInsert::new(table_name, columns, values);
        Ok(stmt)
    }

    /// Extract a literal or placeholder value from a value expression.
    pub(crate) fn expected_expr_value(expr: ExprType) -> RS<ExprValue> {
        match expr {
            ExprType::Value(v) => match &*v {
                ExprItem::ItemValue(expr_v) => match expr_v {
                    ExprValue::ValueLiteral(v) => Ok(ExprValue::ValueLiteral(v.clone())),
                    ExprValue::ValuePlaceholder => Ok(ExprValue::ValuePlaceholder),
                },
                _ => Err(mudu_error!(ErrorCode::InvalidType)),
            },
            _ => Err(mudu_error!(ErrorCode::InvalidType)),
        }
    }

    /// Convert a list of value expressions into literal or placeholder values.
    pub(crate) fn expected_expr_literal_vec(exprs: Vec<ExprType>) -> RS<Vec<ExprValue>> {
        let mut vec = vec![];
        for e in exprs {
            let el = Self::expected_expr_value(e)?;
            vec.push(el);
        }
        Ok(vec)
    }

    /// Parse a typed row value expression list (`VALUES (...), (...)`).
    pub(crate) fn visit_typed_row_value_expr_list(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Vec<Vec<ExprValue>>> {
        let mut cursor = node.walk();
        let mut value_expr_list = vec![];
        let iter = node.children_by_field_name(ts_field_name::LIST, &mut cursor);
        for c in iter {
            let expr_list = self.visit_list(context, c)?;
            let expr_literal = Self::expected_expr_literal_vec(expr_list)?;
            value_expr_list.push(expr_literal);
        }
        Ok(value_expr_list)
    }

    /// Parse the values clause of an INSERT statement.
    pub(crate) fn visit_insert_values(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<(Vec<String>, Vec<Vec<ExprValue>>)> {
        let opt = node.child_by_field_name(ts_field_name::COLUMN_LIST);
        let mut columns = vec![];
        if let Some(c) = opt {
            let mut f = |name: String| {
                columns.push(name);
                Ok::<_, MuduError>(())
            };
            self.visit_column_list(context, c, &mut f)?;
        }

        let opt = node.child_by_field_name(ts_field_name::TYPED_ROW_VALUE_EXPR_LIST);
        let context_str = ts_node_context_string(context.parse_str(), &node)?;
        let n_val_expr_list = rs_of_opt(opt, || {
            mudu_error!(
                ErrorCode::Parse,
                format!("no value expression list node {}", context_str)
            )
        })?;
        let expr_l = self.visit_typed_row_value_expr_list(context, n_val_expr_list)?;
        Ok((columns, expr_l))
    }

    /// Iterate over the column list and invoke a callback for each column name.
    pub(crate) fn visit_column_list<F>(
        &self,
        context: &ParseContext,
        node: Node,
        f: &mut F,
    ) -> RS<()>
    where
        F: FnMut(String) -> RS<()>,
    {
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::COLUMN, &mut cursor);
        for c in iter {
            let column_name = self.visit_column(context, c)?;
            f(column_name)?;
        }
        Ok(())
    }

    /// Parse a list of expressions.
    pub(crate) fn visit_list(&self, context: &ParseContext, node: Node) -> RS<Vec<ExprType>> {
        let mut vec = vec![];
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::EXPRESSION, &mut cursor);
        for n in iter {
            let expr = self.visit_expression(context, n)?;
            vec.push(expr);
        }
        Ok(vec)
    }

    /// Parse a single column identifier.
    pub(crate) fn visit_column(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }
}
