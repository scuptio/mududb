//! Shared binding logic for select-list items (columns and aggregates).
//!
//! Used by both the query binder and the statement describer so that the
//! output tuple description is computed identically on both paths.

use crate::bound_stmt::{AggregateFunc, BoundAggregate, BoundSelectColumn, BoundSelectItem};
use ::catalog::contract::table_desc::TableDesc;
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
use mudu_type::type_family::TypeFamily;
use sql_parser::ast::expr_function::FunctionArg;
use sql_parser::ast::select_term::{SelectField, SelectTerm};

/// Fractional digits added to AVG results over exact numeric inputs.
const AVG_RESULT_SCALE: u8 = 6;

/// Bind the select-list terms of a query against a table, producing the
/// projection items and the output tuple description.
///
/// Without `GROUP BY` a select list is either all plain columns or all
/// aggregates; mixing the two is rejected.
pub(crate) fn bind_select_items(
    table_desc: &TableDesc,
    terms: &[SelectTerm],
) -> RS<(Vec<BoundSelectItem>, TupleFieldDesc)> {
    let has_aggregate = terms
        .iter()
        .any(|term| matches!(term.field(), SelectField::Function(_)));
    let has_column = terms
        .iter()
        .any(|term| matches!(term.field(), SelectField::Column(_)));
    if has_aggregate && has_column {
        return Err(mudu_error!(
            ER::NotImplemented,
            "mixing plain columns and aggregates without GROUP BY is not implemented"
        ));
    }

    let mut items = Vec::with_capacity(terms.len());
    let mut desc_fields = Vec::with_capacity(terms.len());
    for term in terms {
        match term.field() {
            SelectField::Column(name) => {
                let attr = attr_index_by_name(table_desc, name.name())?;
                let field = table_desc.get_attr(attr);
                let output_name = if term.alias().is_empty() {
                    field.name().clone()
                } else {
                    term.alias().clone()
                };
                desc_fields.push(DatumDesc::new_nullable(
                    output_name.clone(),
                    field.type_desc().clone(),
                    field.nullable(),
                ));
                items.push(BoundSelectItem::Column(BoundSelectColumn {
                    attr,
                    output_name,
                }));
            }
            SelectField::Function(function) => {
                let aggregate = bind_aggregate(table_desc, function, term.alias())?;
                desc_fields.push(DatumDesc::new_nullable(
                    aggregate.output_name.clone(),
                    aggregate.result_type.clone(),
                    aggregate.nullable,
                ));
                items.push(BoundSelectItem::Aggregate(aggregate));
            }
        }
    }
    Ok((items, TupleFieldDesc::new(desc_fields)))
}

fn bind_aggregate(
    table_desc: &TableDesc,
    function: &sql_parser::ast::expr_function::ExprFunction,
    alias: &str,
) -> RS<BoundAggregate> {
    let name = function.name().to_lowercase();
    let func = aggregate_func_by_name(function, &name)?;
    let arg = aggregate_arg(table_desc, function, func, &name)?;
    let result_type = aggregate_result_type(table_desc, func, arg, &name)?;

    let output_name = if alias.is_empty() {
        name
    } else {
        alias.to_string()
    };
    Ok(BoundAggregate {
        func,
        arg,
        result_type,
        output_name,
        nullable: func != AggregateFunc::Count,
    })
}

/// Resolves the aggregate kind from the (already lowercased) function name.
fn aggregate_func_by_name(
    function: &sql_parser::ast::expr_function::ExprFunction,
    name: &str,
) -> RS<AggregateFunc> {
    Ok(match name {
        "count" => AggregateFunc::Count,
        "sum" => AggregateFunc::Sum,
        "avg" => AggregateFunc::Avg,
        "min" => AggregateFunc::Min,
        "max" => AggregateFunc::Max,
        _ => {
            return Err(mudu_error!(
                ER::NotImplemented,
                format!("unsupported function {}", function.name())
            ))
        }
    })
}

/// Binds the function argument: `*` is accepted by COUNT only; a column is
/// resolved against the table.
fn aggregate_arg(
    table_desc: &TableDesc,
    function: &sql_parser::ast::expr_function::ExprFunction,
    func: AggregateFunc,
    name: &str,
) -> RS<Option<AttrIndex>> {
    match function.arg() {
        FunctionArg::Star => {
            if func != AggregateFunc::Count {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    format!("function {} does not accept `*` as argument", name)
                ));
            }
            Ok(None)
        }
        FunctionArg::Column(column) => Ok(Some(attr_index_by_name(table_desc, column.name())?)),
    }
}

/// Only COUNT accepts `*`; every other function was validated to have a
/// column argument.
fn require_column_arg(arg: Option<AttrIndex>, name: &str) -> RS<AttrIndex> {
    arg.ok_or_else(|| {
        mudu_error!(
            ER::InvalidState,
            format!("function {} requires a column argument", name)
        )
    })
}

/// Result type of the aggregate, derived from the argument's type.
fn aggregate_result_type(
    table_desc: &TableDesc,
    func: AggregateFunc,
    arg: Option<AttrIndex>,
    name: &str,
) -> RS<DataType> {
    match func {
        AggregateFunc::Count => Ok(DataType::default_for(TypeFamily::I64)),
        AggregateFunc::Min | AggregateFunc::Max => min_max_result_type(table_desc, arg, name),
        AggregateFunc::Sum | AggregateFunc::Avg => sum_avg_result_type(table_desc, func, arg, name),
    }
}

/// MIN/MAX return the argument's own type; only orderable scalar families
/// are supported.
fn min_max_result_type(table_desc: &TableDesc, arg: Option<AttrIndex>, name: &str) -> RS<DataType> {
    let attr = require_column_arg(arg, name)?;
    let field = table_desc.get_attr(attr);
    match field.type_desc().type_family() {
        TypeFamily::I32
        | TypeFamily::I64
        | TypeFamily::F32
        | TypeFamily::F64
        | TypeFamily::Numeric
        | TypeFamily::String => Ok(field.type_desc().clone()),
        family => Err(mudu_error!(
            ER::NotImplemented,
            format!("function {} does not support {:?} arguments", name, family)
        )),
    }
}

/// SUM/AVG widen integers and produce fractional digits for AVG over exact
/// numeric inputs.
fn sum_avg_result_type(
    table_desc: &TableDesc,
    func: AggregateFunc,
    arg: Option<AttrIndex>,
    name: &str,
) -> RS<DataType> {
    let attr = require_column_arg(arg, name)?;
    let field = table_desc.get_attr(attr);
    match field.type_desc().type_family() {
        TypeFamily::I32 | TypeFamily::I64 => Ok(match func {
            AggregateFunc::Sum => DataType::default_for(TypeFamily::I64),
            // AVG over integers yields NUMERIC with fractional
            // digits (a scale-0 NUMERIC would truncate the result).
            _ => DataType::from_numeric(DataTypeParamNumeric::new(38, AVG_RESULT_SCALE)),
        }),
        TypeFamily::F32 | TypeFamily::F64 => Ok(DataType::default_for(TypeFamily::F64)),
        TypeFamily::Numeric => Ok(match func {
            // SUM keeps the argument's precision and scale.
            AggregateFunc::Sum => field.type_desc().clone(),
            _ => {
                let param = field
                    .type_desc()
                    .as_numeric_param()
                    .cloned()
                    .unwrap_or_default();
                let scale = (param.scale() as i64 + AVG_RESULT_SCALE as i64)
                    .min(param.precision() as i64 - 1)
                    .max(0) as u8;
                DataType::from_numeric(DataTypeParamNumeric::new(param.precision(), scale))
            }
        }),
        family => Err(mudu_error!(
            ER::NotImplemented,
            format!(
                "function {} requires a numeric argument, got {:?}",
                name, family
            )
        )),
    }
}

pub(crate) fn attr_index_by_name(table_desc: &TableDesc, name: &str) -> RS<AttrIndex> {
    let total = table_desc.fields().len();
    (0..total)
        .find(|attr| table_desc.get_attr(*attr).name() == name)
        .ok_or_else(|| mudu_error!(ER::EntityNotFound, format!("cannot find column {}", name)))
}
