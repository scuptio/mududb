use crate::ast::expr_arithmetic::ExprArithmetic;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::ExprItem;
use crate::ast::expr_logical::ExprLogical;
use std::sync::Arc;

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
