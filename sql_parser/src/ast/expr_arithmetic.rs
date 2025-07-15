use crate::ast::ast_node::ASTNode;
use crate::ast::expr_operator::Arithmetic;
use crate::ast::expression::ExprType;
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub struct ExprArithmetic {
    op: Arithmetic,
    left: ExprType,
    right: ExprType,
}

impl ExprArithmetic {
    pub fn new(op: Arithmetic, left: ExprType, right: ExprType) -> Self {
        Self { op, left, right }
    }

    pub fn op(&self) -> &Arithmetic {
        &self.op
    }

    pub fn left(&self) -> &ExprType {
        &self.left
    }

    pub fn right(&self) -> &ExprType {
        &self.right
    }
}

impl Debug for ExprArithmetic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "op: ")?;
        self.op.fmt(f)?;
        write!(f, "left: ")?;
        self.left.fmt(f)?;
        write!(f, "right: ")?;
        self.right.fmt(f)?;
        Ok(())
    }
}


impl ASTNode for ExprArithmetic {}
