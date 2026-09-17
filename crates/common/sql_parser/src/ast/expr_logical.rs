use std::fmt::Debug;

use crate::ast::ast_node::ASTNode;
use crate::ast::expr_operator::LogicalConnective;
use crate::ast::expression::ExprType;

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
