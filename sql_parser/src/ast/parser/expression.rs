//! Generic expression parser.

use super::context::ParseContext;
use super::error::ts_node_context_string;
use super::SQLParser;
use crate::ast::expr_arithmetic::ExprArithmetic;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_logical::ExprLogical;
use crate::ast::expr_name::ExprName;
use crate::ast::expr_operator::Operator;
use crate::ast::expression::ExprType;
use crate::ts_const::{ts_field_name, ts_kind_name};
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::data_type::numeric::Numeric;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::dat_typed::DatTyped;
use std::str::FromStr;
use std::sync::Arc;
use tree_sitter::Node;

impl SQLParser {
    /// Parse an expression node into an [`ExprType`].
    pub(crate) fn visit_expression(&self, context: &ParseContext, node: Node) -> RS<ExprType> {
        let opt_binary_expression = node.child_by_field_name(ts_field_name::BINARY_EXPRESSION);
        if let Some(n) = opt_binary_expression {
            return self.visit_binary_expression(context, n);
        }

        let opt_literal = node.child_by_field_name(ts_field_name::LITERAL);
        if let Some(n) = opt_literal {
            let literal = self.visit_literal(context, n)?;
            return Ok(ExprType::Value(Arc::new(ExprItem::ItemValue(
                ExprValue::ValueLiteral(literal),
            ))));
        }

        let opt_qualified_field = node.child_by_field_name(ts_field_name::QUALIFIED_FIELD);
        if let Some(n) = opt_qualified_field {
            let field = self.visit_qualified_field(context, n)?;
            return Ok(ExprType::Value(Arc::new(ExprItem::ItemName(field))));
        }

        let opt_expression = node.child_by_field_name(ts_field_name::EXPRESSION_IN_PARENTHESIS);
        if let Some(n) = opt_expression {
            return self.visit_expression(context, n);
        }

        let opt_place_holder = node.child_by_field_name(ts_field_name::PARAMETER_PLACEHOLDER);
        if let Some(_n) = opt_place_holder {
            return Ok(ExprType::Value(Arc::new(ExprItem::ItemValue(
                ExprValue::ValuePlaceholder,
            ))));
        }
        Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "unknown expression {}",
                ts_node_context_string(context.parse_str(), &node)?
            )
        ))
    }

    /// Parse a literal node into an [`ExprLiteral`].
    pub(crate) fn visit_literal(&self, context: &ParseContext, node: Node) -> RS<ExprLiteral> {
        if node
            .child_by_field_name(ts_field_name::KEYWORD_NULL)
            .is_some()
            || node.kind() == ts_kind_name::S_KEYWORD_NULL
            || ts_node_context_string(context.parse_str(), &node)?.eq_ignore_ascii_case("null")
        {
            return Ok(ExprLiteral::Null);
        }
        let typed = if let Some(n) = node.child_by_field_name("integer") {
            let s = self.visit_integer(context, n)?;
            let i = i64::from_str(s.as_str())
                .map_err(|e| mudu_error!(ErrorCode::Parse, format!("parse integer error: {e}")))?;
            DatTyped::from_i64(i)
        } else if let Some(n) = node.child_by_field_name("decimal") {
            let s = self.visit_decimal(context, n)?;
            let numeric = Numeric::parse(s.as_str())
                .map_err(|e| mudu_error!(ErrorCode::Parse, format!("parse numeric error {}", e)))?;
            DatTyped::from_numeric(numeric)
        } else if let Some(n) = node.child_by_field_name("string") {
            let s = self.visit_string(context, n)?;
            DatTyped::from_string(s)
        } else if let Some(_n) = node.child_by_field_name("keyword_true") {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "boolean literal true"
            ));
        } else if let Some(_n) = node.child_by_field_name("keyword_false") {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "boolean literal false"
            ));
        } else {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unsupported literal {}",
                    ts_node_context_string(context.parse_str(), &node)?
                )
            ));
        };
        Ok(ExprLiteral::DatumLiteral(typed))
    }

    /// Parse a qualified field reference into an [`ExprName`].
    pub(crate) fn visit_qualified_field(&self, context: &ParseContext, node: Node) -> RS<ExprName> {
        let opt = node.child_by_field_name(ts_field_name::IDENTIFIER_NAME);
        let n = rs_option(opt, "")?;
        let name = self.visit_identifier(context, n)?;
        let mut field = ExprName::new();
        field.set_name(name);
        Ok(field)
    }

    /// Parse a binary expression into an [`ExprType`].
    pub(crate) fn visit_binary_expression(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<ExprType> {
        let opt_n_operator = node.child_by_field_name(ts_field_name::OPERATOR);
        let n_operation = rs_option(opt_n_operator, "no operator in binary expression")?;
        let op = self.visit_operator(context, n_operation)?;
        let opt_left = node.child_by_field_name(ts_field_name::LEFT);
        let left = rs_option(opt_left, "no left in binary expression")?;
        let opt_right = node.child_by_field_name(ts_field_name::RIGHT);
        let right = rs_option(opt_right, "no right in binary expression")?;
        let expr_left = self.visit_expression(context, left)?;
        let expr_right = self.visit_expression(context, right)?;
        let expr: ExprType = match op {
            Operator::OValueCompare(c) => {
                let (l, r) = match (expr_left, expr_right) {
                    (ExprType::Value(l), ExprType::Value(r)) => ((*l).clone(), (*r).clone()),
                    _ => return Err(mudu_error!(ErrorCode::NotImplemented)),
                };
                ExprType::Compare(Arc::new(ExprCompare::new(c, l, r)))
            }
            Operator::OLogicalConnective(c) => {
                ExprType::Logical(Arc::new(ExprLogical::new(c, expr_left, expr_right)))
            }
            Operator::OArithmetic(c) => {
                ExprType::Arithmetic(Arc::new(ExprArithmetic::new(c, expr_left, expr_right)))
            }
        };

        Ok(expr)
    }

    /// Parse an operator node into an [`Operator`].
    pub(crate) fn visit_operator(&self, context: &ParseContext, node: Node) -> RS<Operator> {
        let op_string = ts_node_context_string(context.parse_str(), &node)?;
        Operator::from_name(op_string)
    }
}
