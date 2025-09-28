use crate::contract::table_desc::TableDesc;
use mudu::common::buf::Buf as Datum;
use mudu::common::id::OID;
use mudu::common::result::RS;
use sql_parser::ast::expr_compare::ExprCompare;
use sql_parser::ast::expr_literal::ExprLiteral;
use sql_parser::ast::expr_name::ExprName;
use sql_parser::ast::expr_operator::ValueCompare;

fn convert_expr_compare_equal(
    expr: &ExprName,
    expr_literal: &ExprLiteral,
    desc: &TableDesc,
) -> RS<(OID, Datum)> {
    todo!()
}

fn convert_expr_compare(expr: &ExprCompare, desc: &TableDesc) -> RS<(OID, Datum)> {
    match expr.op() {
        ValueCompare::EQ => {
            match (expr.left(), expr.right()) {
                _ => {
                    Err(todo!())
                }
            }
        }
        _ => Err(todo!()),
    }
}

pub fn convert_exprs(exprs: &Vec<ExprCompare>, table_desc: &TableDesc) -> RS<Vec<(OID, Datum)>> {
    let mut vec = vec![];
    for expr in exprs.iter() {
        let datum = convert_expr_compare(expr, table_desc)?;
        vec.push(datum)
    }
    Ok(vec)
}
