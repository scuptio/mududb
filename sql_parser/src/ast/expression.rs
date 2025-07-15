use crate::ast::expr_arithmetic::ExprArithmetic;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::ExprItem;
use crate::ast::expr_logical::ExprLogical;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ExprType {
    Logical(Arc<ExprLogical>),
    Compare(Arc<ExprCompare>),
    Value(Arc<ExprItem>),
    Arithmetic(Arc<ExprArithmetic>)
}
