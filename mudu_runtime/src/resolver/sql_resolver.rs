use crate::resolver::filter::Filter;
use crate::resolver::item_value::ItemValue;
use crate::resolver::resolved_command::ResolvedCommand;
use crate::resolver::resolved_insert::ResolvedInsert;
use crate::resolver::resolved_select::ResolvedSelect;
use crate::resolver::resolved_update::ResolvedUpdate;
use crate::resolver::schema_mgr::SchemaMgr;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::tuple::datum_desc::DatumDesc;
use mudu_gen::src_gen::column_def::TableColumnDef;
use mudu_gen::src_gen::table_def::TableDef;
use sql_parser::ast::expr_compare::ExprCompare;
use sql_parser::ast::expr_item::{ExprItem, ExprValue};
use sql_parser::ast::expr_literal::ExprLiteral;
use sql_parser::ast::stmt_insert::StmtInsert;
use sql_parser::ast::stmt_select::StmtSelect;
use sql_parser::ast::stmt_type::StmtCommand;
use sql_parser::ast::stmt_update::{AssignedValue, StmtUpdate};
use std::sync::Arc;

/// SQLResolver performs analyzing and checking the semantics of a parsed SQL statement
pub struct SQLResolver {
    schema_mgr: SchemaMgr,
}

impl SQLResolver {
    pub fn new(schema_mgr: SchemaMgr) -> Self {
        Self { schema_mgr }
    }

    pub fn resolve_query(&self, stmt: &StmtSelect) -> RS<ResolvedSelect> {
        stmt_select_to_resolved(stmt, &self.schema_mgr)
    }

    pub fn resolved_command(&self, stmt: &StmtCommand) -> RS<Arc<dyn ResolvedCommand>> {
        let resolved_command: Arc<dyn ResolvedCommand> = match stmt {
            StmtCommand::Update(update) => {
                let update = self.resolve_update(update)?;
                Arc::new(update)
            }
            StmtCommand::Insert(insert) => {
                let insert = self.resolve_insert(insert)?;
                Arc::new(insert)
            }
            _ => {
                panic!("unsupported command statement {:?}", stmt);
            }
        };
        Ok(resolved_command)
    }

    fn resolve_update(&self, stmt: &StmtUpdate) -> RS<ResolvedUpdate> {
        let table_name = stmt.get_table_reference().clone();
        let table_def = self.get_table(&table_name)?;
        let mut vec_set_value = vec![];
        let mut vec_predicate = vec![];
        let mut vec_placeholder = vec![];

        for assignment in stmt.get_set_values() {
            let column_name = assignment.get_column_reference();
            let value = assignment.get_set_value();
            let opt_column_def = table_def.find_column_def_by_name(column_name);
            let column_def = rs_option(opt_column_def, "no such column")?;
            let desc = DatumDesc::new(
                column_def.column_name().clone(),
                column_def.data_type().clone(),
            );
            match value {
                AssignedValue::Expression(_) => {
                    // todo set value expression could be > 1 placeholder, to fix it ...
                    vec_set_value.push((desc.clone(), ItemValue::Placeholder));
                    vec_placeholder.push(desc);
                }
                AssignedValue::Value(v) => match v {
                    ExprValue::ValueLiteral(v_l) => {
                        vec_set_value.push((desc, ItemValue::Literal(v_l.dat_type().clone())));
                    }
                    ExprValue::ValuePlaceholder => {
                        vec_set_value.push((desc.clone(), ItemValue::Placeholder));
                        vec_placeholder.push(desc);
                    }
                },
            }
        }

        real_where_predicate(
            &table_def,
            stmt.get_where_predicate(),
            &mut vec_predicate,
            &mut vec_placeholder,
        )?;
        let r_update = ResolvedUpdate::new(
            table_name,
            vec_set_value,
            vec_predicate,
            vec![],
            vec_placeholder,
        );
        Ok(r_update)
    }

    fn resolve_insert(&self, stmt: &StmtInsert) -> RS<ResolvedInsert> {
        let table_def = self.get_table(stmt.table_name())?;

        let mut vec_column_def = vec![];
        let value_columns = if stmt.columns().is_empty() {
            table_def.table_columns()
        } else {
            for column_name in stmt.columns() {
                let col = Self::get_column(&table_def, column_name)?;
                vec_column_def.push(col.clone());
            }
            &vec_column_def
        };

        for value in stmt.values_list().iter() {
            if value_columns.len() != value.len() {
                return Err(m_error!(
                    EC::ParseErr,
                    format!("column and value size are not equal {:?}", stmt)
                ));
            }
        }
        if stmt.values_list().len() != 1 {
            return Err(m_error!(EC::MuduError, "only support 1 row insert"));
        }
        let mut insert_values = vec![];
        let mut placeholder = vec![];
        let row = &stmt.values_list()[0];
        for (i, v) in row.iter().enumerate() {
            let c_def = &value_columns[i];
            let desc = DatumDesc::new(c_def.column_name().clone(), c_def.data_type().clone());

            let item_value = match v {
                ExprValue::ValueLiteral(l) => ItemValue::Literal(l.dat_type().clone()),
                ExprValue::ValuePlaceholder => {
                    placeholder.push(desc.clone());
                    ItemValue::Placeholder
                }
            };
            insert_values.push((desc, item_value));
        }
        Ok(ResolvedInsert::new(insert_values, placeholder))
    }

    fn get_column<'a>(table_def: &'a TableDef, column_name: &String) -> RS<&'a TableColumnDef> {
        let opt_column_def = table_def.find_column_def_by_name(column_name);
        let column_def = rs_option(
            opt_column_def,
            &format!("no such column named {}", column_name),
        )?;
        Ok(column_def)
    }

    fn get_table(&self, table_name: &String) -> RS<TableDef> {
        let opt_table_def = self.schema_mgr.get(table_name)?;
        let table_def = rs_option(
            opt_table_def,
            &format!("no such table named {}", table_name),
        )?;
        Ok(table_def)
    }
}

fn build_data_desc_for_name(column_name: &str, table_def: &TableDef) -> RS<DatumDesc> {
    let opt = table_def.find_column_def_by_name(column_name);
    let column_def = rs_option(opt, &format!("no such column {}", column_name))?;
    let datum_desc = DatumDesc::new(column_name.to_string(), column_def.data_type().clone());
    Ok(datum_desc)
}

fn real_where_predicate(
    table_def: &TableDef,
    expr_compare_list: &Vec<ExprCompare>,
    vec_predicate: &mut Vec<(DatumDesc, Filter)>,
    vec_placeholder: &mut Vec<DatumDesc>,
) -> RS<()> {
    for predicate in expr_compare_list {
        let right = predicate.right();
        let left = predicate.left();
        match (left, right) {
            (ExprItem::ItemName(expr_name), ExprItem::ItemValue(value)) => match value {
                ExprValue::ValueLiteral(literal) => {
                    let datum_desc = build_data_desc_for_name(expr_name.name(), &table_def)?;
                    let literal_value = match literal {
                        ExprLiteral::DatumLiteral(typed) => typed.clone(),
                    };
                    let filter = Filter::new(*predicate.op(), ItemValue::Literal(literal_value));
                    vec_predicate.push((datum_desc, filter));
                }
                ExprValue::ValuePlaceholder => {
                    let datum_desc = build_data_desc_for_name(expr_name.name(), &table_def)?;
                    let filter = Filter::new(*predicate.op(), ItemValue::Placeholder);
                    vec_predicate.push((datum_desc.clone(), filter));
                    vec_placeholder.push(datum_desc);
                }
            },
            (_, _) => {
                return Err(m_error!(
                    EC::ParseErr,
                    format!(
                        "\
In where filter, the left must be name, \
the right must be a placeholder or literal value,\
but got {:?} {:?}\
                ",
                        left, right
                    )
                ));
            }
        }
    }
    Ok(())
}

fn stmt_select_to_resolved(stmt: &StmtSelect, schema_mgr: &SchemaMgr) -> RS<ResolvedSelect> {
    let table_name = stmt.get_table_reference();
    let opt = schema_mgr.get(table_name)?;
    let mut vec_projection = vec![];

    let table_def = rs_option(opt, &format!("no such table {}", table_name))?;
    for term in stmt.get_select_term_list() {
        let datum_desc = build_data_desc_for_name(term.field().name(), &table_def)?;
        vec_projection.push(datum_desc);
    }
    let mut vec_predicate = vec![];
    let mut vec_placeholder = vec![];
    real_where_predicate(
        &table_def,
        stmt.get_where_predicate(),
        &mut vec_predicate,
        &mut vec_placeholder,
    )?;

    let rs = ResolvedSelect::new(
        table_name.clone(),
        vec_projection,
        vec_predicate,
        vec![],
        vec_placeholder,
    );
    Ok(rs)
}
