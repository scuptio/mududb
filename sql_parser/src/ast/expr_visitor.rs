//! Visitor helpers for extracting comparison expressions.

use crate::ast::expr_compare::ExprCompare;
use crate::ast::expression::ExprType;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// Helper for traversing expression trees.
pub struct ExprVisitor {}

impl ExprVisitor {
    /// Recursively extract [`ExprCompare`] nodes from a logical or comparison
    /// expression tree.
    ///
    /// Returns an error if the expression contains an unsupported node type.
    pub fn extract_expr_compare_list(expr: ExprType, vec: &mut Vec<ExprCompare>) -> RS<()> {
        match expr {
            ExprType::Logical(expr_logical) => {
                let left = expr_logical.left().clone();
                let right = expr_logical.right().clone();
                Self::extract_expr_compare_list(left, vec)?;
                Self::extract_expr_compare_list(right, vec)?;
            }
            ExprType::Compare(expr) => {
                vec.push((*expr).clone());
            }
            _ => {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "expected compare type or logical type"
                ));
            }
        }
        Ok(())
    }
}
