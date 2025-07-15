use std::fmt::Debug;

use crate::ast::ast_node::ASTNode;
use crate::ast::expr_operator::LogicalConnective;
use crate::ast::expression::ExprType;

#[derive(Clone, Debug)]
pub struct ExprLogical {
    op: LogicalConnective,
    left: ExprType,
    right: ExprType,
}

impl ExprLogical {
    pub fn new(op: LogicalConnective, left: ExprType, right: ExprType) -> Self {
        Self { op, left, right }
    }

    pub fn op(&self) -> &LogicalConnective {
        &self.op
    }

    pub fn left(&self) -> &ExprType {
        &self.left
    }

    pub fn right(&self) -> &ExprType {
        &self.right
    }

    pub fn into_left(self) -> ExprType {
        self.left
    }

    pub fn into_right(self) -> ExprType {
        self.right
    }
}

impl ASTNode for ExprLogical {}
