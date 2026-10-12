//! DML template binding methods of the `Binder` impl (SELECT/INSERT/UPDATE/DELETE).
use super::*;

impl Binder {
    pub(super) async fn bind_select_template(
        &self,
        stmt: &sql_parser::ast::stmt_select::StmtSelect,
        recorder: &mut SlotRecorder,
    ) -> RS<SelectTemplate> {
        let table_desc = self.resolve_table(stmt.get_table_reference()).await?;
        let (select_items, tuple_desc) =
            crate::select_projection::bind_select_items(&table_desc, stmt.get_select_term_list())?;
        let (predicate, residual) =
            self.bind_predicate_template(&table_desc, stmt.get_where_predicate(), recorder)?;
        Ok(SelectTemplate {
            table_id: table_desc.id(),
            select_items,
            tuple_desc,
            predicate,
            residual,
            has_fs_columns: has_fs_bound_columns(&table_desc),
        })
    }

    pub(super) async fn bind_insert_template(
        &self,
        stmt: &StmtInsert,
        recorder: &mut SlotRecorder,
    ) -> RS<InsertTemplate> {
        let table_desc = self.resolve_table(stmt.table_reference()).await?;

        let columns = if stmt.columns().is_empty() {
            let total = table_desc.fields().len();
            (0..total)
                .map(|attr| table_desc.get_attr(attr).name().clone())
                .collect::<Vec<_>>()
        } else {
            stmt.columns().clone()
        };

        let mut rows = Vec::with_capacity(stmt.values_list().len());
        for values in stmt.values_list() {
            if columns.len() != values.len() {
                return Err(mudu_error!(
                    ER::InvalidArgument,
                    "insert column size mismatch"
                ));
            }

            let mut key = vec![];
            let mut value = vec![];
            for (name, expr) in columns.iter().zip(values.iter()) {
                let attr = self.attr_index_by_name(&table_desc, name)?;
                let field = table_desc.get_attr(attr);
                let datum = template_from_expr(expr, field.type_desc(), recorder)?;
                if matches!(datum, TemplateDatum::Const(None)) && !field.nullable() {
                    return Err(mudu_error!(
                        ER::InvalidTuple,
                        format!("cannot insert NULL into NOT NULL column {}", field.name())
                    ));
                }
                if field.primary_index().is_some() {
                    match datum {
                        TemplateDatum::Const(None) => {
                            return Err(mudu_error!(
                                ER::InvalidTuple,
                                format!("cannot insert NULL into key column {}", field.name())
                            ))
                        }
                        datum => key.push((attr, datum)),
                    }
                } else if !matches!(datum, TemplateDatum::Const(None)) {
                    value.push((attr, datum));
                }
            }
            rows.push(InsertRowTemplate { key, value });
        }

        Ok(InsertTemplate {
            table_id: table_desc.id(),
            rows,
            has_fs_columns: has_fs_bound_columns(&table_desc),
        })
    }

    pub(super) async fn bind_update_template(
        &self,
        stmt: &StmtUpdate,
        recorder: &mut SlotRecorder,
    ) -> RS<UpdateTemplate> {
        let table_desc = self.resolve_table(stmt.get_table_reference()).await?;
        let mut value = Vec::with_capacity(stmt.get_set_values().len());

        for assignment in stmt.get_set_values() {
            let attr = self.attr_index_by_name(&table_desc, assignment.get_column_reference())?;
            let field = table_desc.get_attr(attr);
            if field.primary_index().is_some() {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    "updating primary key columns is not implemented"
                ));
            }
            match assignment.get_set_value() {
                AssignedValue::Value(expr) => {
                    let datum = template_from_expr(expr, field.type_desc(), recorder)?;
                    if matches!(datum, TemplateDatum::Const(None)) && !field.nullable() {
                        return Err(mudu_error!(
                            ER::InvalidTuple,
                            format!("cannot update NOT NULL column {} to NULL", field.name())
                        ));
                    }
                    match datum {
                        // Assigning NULL to an FS-bound column marks it for rebinding:
                        // the DML hook assigns a fresh system object id (an empty
                        // datum is the "touched, system-assigned" sentinel).
                        TemplateDatum::Const(None) if field.fs_binding().is_some() => value.push((
                            attr,
                            SetValueTemplate::Absolute(TemplateDatum::Const(Some(Vec::new()))),
                        )),
                        datum => value.push((attr, SetValueTemplate::Absolute(datum))),
                    }
                }
                AssignedValue::Expression(expr) => {
                    let set_value = Self::bind_delta_assignment_template(
                        assignment.get_column_reference(),
                        expr,
                        field.type_desc(),
                        recorder,
                    )?;
                    value.push((attr, set_value));
                }
            }
        }
        let key =
            self.bind_exact_key_template(&table_desc, stmt.get_where_predicate(), recorder)?;

        Ok(UpdateTemplate {
            table_id: table_desc.id(),
            key,
            value,
            has_fs_columns: has_fs_bound_columns(&table_desc),
        })
    }

    /// Bind the restricted expression assignment `SET col = col <+|->
    /// <integer>` where the right operand is an integer literal or a parameter
    /// placeholder (`?`).
    ///
    /// The left operand must reference the assigned column itself and the
    /// right operand must be a signed/unsigned integer literal or a parameter
    /// of an integer type family; the assigned column must be an integer or
    /// numeric column. Any other expression form is rejected as
    /// `NotImplemented`. The operand is stored in the column's binary format
    /// (coerced for numeric columns) so the executor can decode it with the
    /// column type. A placeholder is recorded as a delta-operand slot; its
    /// type-family check and encoding run at fill time through the same path
    /// as a plain `SET col = ?` assignment (identical to immediate binding,
    /// where both run at bind time).
    fn bind_delta_assignment_template(
        column_reference: &str,
        expr: &ExprType,
        column_type: &DataType,
        recorder: &mut SlotRecorder,
    ) -> RS<SetValueTemplate> {
        let not_implemented = || {
            mudu_error!(
                ER::NotImplemented,
                "expression updates are not implemented \
                 (only `SET col = col +|- <integer literal or ?>` is supported)"
            )
        };
        let ExprType::Arithmetic(arithmetic) = expr else {
            return Err(not_implemented());
        };
        let op = match arithmetic.op() {
            Arithmetic::PLUS => DeltaOp::Add,
            Arithmetic::MINUS => DeltaOp::Sub,
            _ => return Err(not_implemented()),
        };
        let ExprType::Value(left) = arithmetic.left() else {
            return Err(not_implemented());
        };
        let ExprItem::ItemName(name) = left.as_ref() else {
            return Err(not_implemented());
        };
        if name.name() != column_reference {
            return Err(not_implemented());
        }
        match column_type.type_family() {
            // Numeric columns (e.g. TPC-C money columns) accept an integer
            // delta operand; the operand is coerced to the column's numeric
            // layout when encoded.
            TypeFamily::I32
            | TypeFamily::I64
            | TypeFamily::I128
            | TypeFamily::U128
            | TypeFamily::Numeric => {}
            _ => return Err(not_implemented()),
        }
        let ExprType::Value(right) = arithmetic.right() else {
            return Err(not_implemented());
        };
        let ExprItem::ItemValue(value) = right.as_ref() else {
            return Err(not_implemented());
        };
        let operand = match value {
            ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(literal)) => {
                match literal.data_type().type_family() {
                    TypeFamily::I32 | TypeFamily::I64 | TypeFamily::I128 | TypeFamily::U128 => {}
                    // Drivers that substitute params client-side send numeric
                    // delta operands as quoted string literals; ValueCodec
                    // coerces them to the column's numeric layout.
                    TypeFamily::String if column_type.type_family() == TypeFamily::Numeric => {}
                    _ => return Err(not_implemented()),
                }
                let Some(binary) = ValueCodec::binary_from_literal(
                    &ExprLiteral::DatumLiteral(literal.clone()),
                    column_type,
                )?
                else {
                    // A datum literal never binds to NULL.
                    return Err(not_implemented());
                };
                TemplateDatum::Const(Some(binary))
            }
            ExprValue::ValuePlaceholder => {
                // The placeholder family check (integer families, or String
                // for numeric columns whose params arrive type-erased as
                // strings) runs at fill time in `fill_slot`, which applies
                // the same rules and then encodes through the same path as a
                // plain `SET col = ?` assignment.
                TemplateDatum::Slot(recorder.push(column_type.clone(), true))
            }
            _ => return Err(not_implemented()),
        };
        Ok(SetValueTemplate::Delta { op, operand })
    }

    pub(super) async fn bind_delete_template(
        &self,
        stmt: &StmtDelete,
        recorder: &mut SlotRecorder,
    ) -> RS<DeleteTemplate> {
        let table_desc = self.resolve_table(stmt.get_table_reference()).await?;
        let key =
            self.bind_exact_key_template(&table_desc, stmt.get_where_predicate(), recorder)?;
        Ok(DeleteTemplate {
            table_id: table_desc.id(),
            key,
        })
    }

    fn bind_predicate_template(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        recorder: &mut SlotRecorder,
    ) -> RS<(PredicateTemplate, Vec<ResidualTemplate>)> {
        if predicates.is_empty() {
            return Ok((PredicateTemplate::True, Vec::new()));
        }

        let mut eq_items = vec![];
        let mut start: Bound<Vec<(usize, TemplateDatum)>> = Bound::Unbounded;
        let mut end: Bound<Vec<(usize, TemplateDatum)>> = Bound::Unbounded;
        let mut residual = Vec::new();

        for predicate in predicates {
            let (field_name, expr_value, op) =
                self.field_literal_compare(predicate).ok_or_else(|| {
                    mudu_error!(
                        ER::NotImplemented,
                        "only column/literal predicates are supported"
                    )
                })?;
            let attr = self.attr_index_by_name(table_desc, field_name)?;
            let field = table_desc.get_attr(attr);
            let datum = template_from_expr(&expr_value, field.type_desc(), recorder)?;
            if field.primary_index().is_none() {
                // Non-key predicate: evaluate it row-by-row in the executor
                // layer as a residual filter after the key access.
                residual.push(ResidualTemplate {
                    attr,
                    op,
                    literal: datum,
                });
                continue;
            }
            if matches!(datum, TemplateDatum::Const(None)) {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    "NULL key predicates are not implemented; use IS NULL"
                ));
            }
            match op {
                ValueCompare::EQ => eq_items.push((attr, datum)),
                ValueCompare::GE => start = Bound::Included(vec![(attr, datum)]),
                ValueCompare::GT => start = Bound::Excluded(vec![(attr, datum)]),
                ValueCompare::LE => end = Bound::Included(vec![(attr, datum)]),
                ValueCompare::LT => end = Bound::Excluded(vec![(attr, datum)]),
                ValueCompare::NE => {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "not-equal predicates are not implemented"
                    ))
                }
            }
        }

        let predicate = self.combine_key_predicate_template(table_desc, eq_items, start, end)?;
        Ok((predicate, residual))
    }

    fn combine_key_predicate_template(
        &self,
        table_desc: &TableDesc,
        mut eq_items: Vec<(usize, TemplateDatum)>,
        start: Bound<Vec<(usize, TemplateDatum)>>,
        end: Bound<Vec<(usize, TemplateDatum)>>,
    ) -> RS<PredicateTemplate> {
        if !eq_items.is_empty()
            && matches!(start, Bound::Unbounded)
            && matches!(end, Bound::Unbounded)
        {
            eq_items.sort_by_key(|(attr, _)| {
                table_desc
                    .get_attr(*attr)
                    .primary_index()
                    .unwrap_or_default()
            });
            for (index, (attr, _)) in eq_items.iter().enumerate() {
                if table_desc.get_attr(*attr).primary_index() != Some(index) {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "select equality predicates on primary keys must cover a left prefix of the primary key"
                    ));
                }
            }
            if eq_items.len() == table_desc.key_indices().len() {
                return Ok(PredicateTemplate::KeyEq { key: eq_items });
            }
            return Ok(PredicateTemplate::KeyPrefixEq { prefix: eq_items });
        }

        if !eq_items.is_empty() {
            return Err(mudu_error!(
                ER::NotImplemented,
                "mixed equality and range predicates are not implemented"
            ));
        }

        Ok(PredicateTemplate::KeyRange { start, end })
    }

    fn bind_exact_key_template(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        recorder: &mut SlotRecorder,
    ) -> RS<Vec<(usize, TemplateDatum)>> {
        let (predicate, residual) =
            self.bind_predicate_template(table_desc, predicates, recorder)?;
        if !residual.is_empty() {
            return Err(mudu_error!(
                ER::NotImplemented,
                "non-key predicates are not implemented"
            ));
        }
        match predicate {
            PredicateTemplate::KeyEq { mut key } => {
                if key.len() != table_desc.key_indices().len() {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "update/delete require a complete primary key predicate"
                    ));
                }
                key.sort_by_key(|(attr, _)| {
                    table_desc
                        .get_attr(*attr)
                        .primary_index()
                        .unwrap_or_default()
                });
                for (index, (attr, _)) in key.iter().enumerate() {
                    if table_desc.get_attr(*attr).primary_index() != Some(index) {
                        return Err(mudu_error!(
                            ER::NotImplemented,
                            "update/delete require one equality predicate for each primary key column"
                        ));
                    }
                }
                Ok(key)
            }
            PredicateTemplate::KeyPrefixEq { .. } => Err(mudu_error!(
                ER::NotImplemented,
                "update/delete require a complete primary key predicate"
            )),
            PredicateTemplate::True => Err(mudu_error!(
                ER::NotImplemented,
                "full-table update/delete is not implemented"
            )),
            PredicateTemplate::KeyRange { .. } => Err(mudu_error!(
                ER::NotImplemented,
                "range update/delete is not implemented"
            )),
        }
    }

    fn field_literal_compare<'a>(
        &self,
        predicate: &'a ExprCompare,
    ) -> Option<(&'a String, ExprValue, ValueCompare)> {
        match (predicate.left(), predicate.right()) {
            (ExprItem::ItemName(name), ExprItem::ItemValue(value)) => {
                Some((name.name(), value.clone(), *predicate.op()))
            }
            (ExprItem::ItemValue(value), ExprItem::ItemName(name)) => Some((
                name.name(),
                value.clone(),
                Self::reverse_compare(*predicate.op()),
            )),
            _ => None,
        }
    }

    fn reverse_compare(op: ValueCompare) -> ValueCompare {
        ValueCompare::revert_cmp_op(op)
    }
}
