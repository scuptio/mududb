use crate::ast::expr_compare::ExprCompare;
use crate::ast::expression::ExprType;

pub struct ExprVisitor {}

impl ExprVisitor {
    pub fn extract_expr_compare_list(expr: ExprType, vec: &mut Vec<ExprCompare>) {
        match expr {
            ExprType::Logical(expr_logical) => {
                let left = expr_logical.left().clone();
                let right = expr_logical.right().clone();
                Self::extract_expr_compare_list(left, vec);
                Self::extract_expr_compare_list(right, vec);
            }
            ExprType::Compare(expr) => {
                vec.push((*expr).clone());
            }
            _ => {
                panic!("expected compare type or logical type");
            }
        }
    }
}
