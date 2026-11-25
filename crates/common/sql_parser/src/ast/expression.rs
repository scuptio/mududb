use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::ExprItem;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use crate::ast::ast_node::ASTNode;
use crate::ast::expr_operator::{Arithmetic, LogicalConnective};

/// Top-level expression enum.
#[derive(Clone, Debug)]
pub enum ExprType {
    /// Logical connective expression (`AND`).
    Logical(Arc<ExprLogical>),
    /// Comparison expression (`=`, `<`, `>`, etc.).
    Compare(Arc<ExprCompare>),
    /// Atomic value expression (name, literal, or placeholder).
    Value(Arc<ExprItem>),
    /// Arithmetic expression (`+`, `-`, `*`, `/`).
    Arithmetic(Arc<ExprArithmetic>),
}

/// Arithmetic expression (`+`, `-`, `*`, `/`) with left and right operands.
#[derive(Clone)]
pub struct ExprArithmetic {
    op: Arithmetic,
    left: ExprType,
    right: ExprType,
}

impl ExprArithmetic {
    /// Create a new arithmetic expression.
    pub fn new(op: Arithmetic, left: ExprType, right: ExprType) -> Self {
        Self { op, left, right }
    }

    /// Return the arithmetic operator.
    pub fn op(&self) -> &Arithmetic {
        &self.op
    }

    /// Return the left operand.
    pub fn left(&self) -> &ExprType {
        &self.left
    }

    /// Return the right operand.
    pub fn right(&self) -> &ExprType {
        &self.right
    }
}

impl Debug for ExprArithmetic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "arithmetic op: ")?;
        self.op.fmt(f)?;
        write!(f, "left: ")?;
        self.left.fmt(f)?;
        write!(f, "right: ")?;
        self.right.fmt(f)?;
        Ok(())
    }
}

impl ASTNode for ExprArithmetic {}

#[cfg(test)]
mod tests {
    use super::{ExprArithmetic, ExprType};
    use crate::ast::expr_item::{ExprItem, ExprValue};
    use crate::ast::expr_literal::ExprLiteral;
    use crate::ast::expr_operator::Arithmetic;
    use mudu_type::data_typed::DataTyped;
    use std::sync::Arc;

    #[test]
    fn arithmetic_expression_preserves_operands_and_operator() {
        let left = ExprType::Value(Arc::new(ExprItem::ItemValue(ExprValue::ValueLiteral(
            ExprLiteral::DatumLiteral(DataTyped::from_i32(1)),
        ))));
        let right = ExprType::Value(Arc::new(ExprItem::ItemValue(ExprValue::ValueLiteral(
            ExprLiteral::DatumLiteral(DataTyped::from_i32(2)),
        ))));
        let expr = ExprArithmetic::new(Arithmetic::PLUS, left.clone(), right.clone());

        assert!(matches!(expr.op(), Arithmetic::PLUS));
        assert!(matches!(expr.left(), ExprType::Value(_)));
        assert!(matches!(expr.right(), ExprType::Value(_)));

        let debug = format!("{expr:?}");
        assert!(debug.contains("arithmetic op"));
    }
}

/// Logical connective expression (`AND`) with left and right operands.
#[derive(Clone, Debug)]
pub struct ExprLogical {
    op: LogicalConnective,
    left: ExprType,
    right: ExprType,
}

impl ExprLogical {
    /// Create a new logical expression.
    pub fn new(op: LogicalConnective, left: ExprType, right: ExprType) -> Self {
        Self { op, left, right }
    }

    /// Return the logical connective operator.
    pub fn op(&self) -> &LogicalConnective {
        &self.op
    }

    /// Return the left operand.
    pub fn left(&self) -> &ExprType {
        &self.left
    }

    /// Return the right operand.
    pub fn right(&self) -> &ExprType {
        &self.right
    }

    /// Consume the expression and return the left operand.
    pub fn into_left(self) -> ExprType {
        self.left
    }

    /// Consume the expression and return the right operand.
    pub fn into_right(self) -> ExprType {
        self.right
    }
}

impl ASTNode for ExprLogical {}
