use crate::ast::ast_node::ASTNode;
use crate::ast::expr_operator::Arithmetic;
use crate::ast::expression::ExprType;
use std::fmt::{Debug, Formatter};

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
    use super::ExprArithmetic;
    use crate::ast::expr_item::{ExprItem, ExprValue};
    use crate::ast::expr_literal::ExprLiteral;
    use crate::ast::expr_operator::Arithmetic;
    use crate::ast::expression::ExprType;
    use mudu_type::dat_typed::DatTyped;
    use std::sync::Arc;

    #[test]
    fn arithmetic_expression_preserves_operands_and_operator() {
        let left = ExprType::Value(Arc::new(ExprItem::ItemValue(ExprValue::ValueLiteral(
            ExprLiteral::DatumLiteral(DatTyped::from_i32(1)),
        ))));
        let right = ExprType::Value(Arc::new(ExprItem::ItemValue(ExprValue::ValueLiteral(
            ExprLiteral::DatumLiteral(DatTyped::from_i32(2)),
        ))));
        let expr = ExprArithmetic::new(Arithmetic::PLUS, left.clone(), right.clone());

        assert!(matches!(expr.op(), Arithmetic::PLUS));
        assert!(matches!(expr.left(), ExprType::Value(_)));
        assert!(matches!(expr.right(), ExprType::Value(_)));

        let debug = format!("{expr:?}");
        assert!(debug.contains("arithmetic op"));
    }
}
