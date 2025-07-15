use std::fmt::{Debug, Formatter};

use crate::ast::ast_node::ASTNode;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_name::ExprName;

use crate::ast::expr_operator::ValueCompare;

// currently, we only support a ExprField compare with ExprLiteral
#[derive(Clone)]
pub struct ExprCompare {
    op: ValueCompare,
    left: ExprItem,
    right: ExprItem,
}

impl ExprCompare {
    pub fn new(op: ValueCompare, left: ExprItem, right: ExprItem) -> Self {
        Self { op, left, right }
    }

    pub fn op(&self) -> &ValueCompare {
        &self.op
    }

    pub fn left(&self) -> &ExprItem {
        &self.left
    }

    pub fn right(&self) -> &ExprItem {
        &self.right
    }

    pub fn expr_field_op_literal(&self) -> Option<(ExprName, ExprLiteral, ValueCompare)> {
        match (&self.left, &self.right) {
            (ExprItem::ItemName(_l), ExprItem::ItemValue(ExprValue::ValueLiteral(_r))) => {}
            (ExprItem::ItemValue(ExprValue::ValueLiteral(l)), ExprItem::ItemName(r)) => {
                return Some((r.clone(), l.clone(), Self::revert_cmp_op(self.op)));
            }
            _ => {
                return None;
            }
        }
        None
    }

    //
    fn revert_cmp_op(op: ValueCompare) -> ValueCompare {
        match op {
            ValueCompare::EQ => ValueCompare::EQ,
            ValueCompare::LE => ValueCompare::GT,
            ValueCompare::LT => ValueCompare::GE,
            ValueCompare::GE => ValueCompare::LT,
            ValueCompare::GT => ValueCompare::LE,
            ValueCompare::NE => ValueCompare::NE,
        }
    }
}

impl Debug for ExprCompare {
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

impl ASTNode for ExprCompare {}
