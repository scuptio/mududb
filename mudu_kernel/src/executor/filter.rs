//! Residual (non-key) filter executor.
//!
//! Wraps a child executor and evaluates non-key predicates row-by-row in
//! memory. Rows failing any predicate (including SQL UNKNOWN results from
//! NULL comparisons) are skipped; surviving rows are projected down to the
//! output columns.

use crate::contract::query_exec::QueryExec;
use crate::executor::value_compare::compare_values;
use crate::x_engine::api::TupleRow;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::typed_bin::TypedBin;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::datum::DatumDyn;
use sql_parser::ast::expr_operator::ValueCompare;
use std::cmp::Ordering;
use std::sync::Arc;

/// A residual predicate resolved against the child executor's row layout.
pub struct ResidualFilter {
    /// Position of the filtered column in the child row.
    pub input_pos: usize,
    /// Type of the filtered column (used to decode both sides).
    pub data_type: DataType,
    pub op: ValueCompare,
    /// Literal encoded in the column's binary format; `None` means the
    /// literal is NULL, which makes every comparison UNKNOWN.
    pub literal: Option<Vec<u8>>,
}

pub struct FilterExec {
    tuple_desc: TupleFieldDesc,
    child: Arc<dyn QueryExec>,
    filters: Vec<ResidualFilter>,
    /// Output column positions within the child row, in output order.
    projection: Vec<usize>,
}

impl FilterExec {
    pub fn new(
        tuple_desc: TupleFieldDesc,
        child: Arc<dyn QueryExec>,
        filters: Vec<ResidualFilter>,
        projection: Vec<usize>,
    ) -> Self {
        Self {
            tuple_desc,
            child,
            filters,
            projection,
        }
    }

    fn row_matches(&self, row: &TupleRow) -> RS<bool> {
        for filter in &self.filters {
            let field = row.fields().get(filter.input_pos).ok_or_else(|| {
                mudu_error!(ER::InvalidState, "residual filter column out of row bounds")
            })?;
            let Some(binary) = field else {
                // NULL column value: every comparison is UNKNOWN.
                return Ok(false);
            };
            let Some(literal) = &filter.literal else {
                // NULL literal: every comparison is UNKNOWN.
                return Ok(false);
            };
            let value = TypedBin::new(filter.data_type.type_family(), binary.clone())
                .to_value(&filter.data_type)?;
            let literal_value = TypedBin::new(filter.data_type.type_family(), literal.clone())
                .to_value(&filter.data_type)?;
            let Some(ordering) = compare_values(&value, &literal_value)? else {
                return Ok(false);
            };
            let matches = match filter.op {
                ValueCompare::EQ => ordering == Ordering::Equal,
                ValueCompare::NE => ordering != Ordering::Equal,
                ValueCompare::LT => ordering == Ordering::Less,
                ValueCompare::LE => ordering != Ordering::Greater,
                ValueCompare::GT => ordering == Ordering::Greater,
                ValueCompare::GE => ordering != Ordering::Less,
            };
            if !matches {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[async_trait]
impl QueryExec for FilterExec {
    async fn open(&self) -> RS<()> {
        self.child.open().await
    }

    async fn next(&self) -> RS<Option<TupleRow>> {
        while let Some(row) = self.child.next().await? {
            if self.row_matches(&row)? {
                let projected = self
                    .projection
                    .iter()
                    .map(|pos| {
                        row.fields().get(*pos).cloned().ok_or_else(|| {
                            mudu_error!(ER::InvalidState, "projection column out of row bounds")
                        })
                    })
                    .collect::<RS<Vec<_>>>()?;
                return Ok(Some(TupleRow::new_nullable(projected)));
            }
        }
        Ok(None)
    }

    fn tuple_desc(&self) -> RS<TupleFieldDesc> {
        Ok(self.tuple_desc.clone())
    }
}

unsafe impl Send for FilterExec {}

unsafe impl Sync for FilterExec {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_sys::sync::SMutex;
    use mudu_type::type_family::TypeFamily;
    use std::collections::VecDeque;

    struct TestQueryExec {
        rows: SMutex<VecDeque<TupleRow>>,
    }

    #[async_trait]
    impl QueryExec for TestQueryExec {
        async fn open(&self) -> RS<()> {
            Ok(())
        }

        async fn next(&self) -> RS<Option<TupleRow>> {
            Ok(self.rows.lock().unwrap().pop_front())
        }

        fn tuple_desc(&self) -> RS<TupleFieldDesc> {
            Ok(TupleFieldDesc::new(vec![]))
        }
    }

    fn i32_type() -> DataType {
        DataType::default_for(TypeFamily::I32)
    }

    fn i32_bin(value: i32) -> Vec<u8> {
        mudu_type::data_value::DataValue::from_i32(value)
            .to_binary(&i32_type())
            .unwrap()
            .into()
    }

    fn exec_with_rows(rows: Vec<TupleRow>) -> Arc<dyn QueryExec> {
        Arc::new(TestQueryExec {
            rows: SMutex::new(rows.into()),
        })
    }

    fn desc() -> TupleFieldDesc {
        TupleFieldDesc::new(vec![DatumDesc::new_nullable(
            "a".to_string(),
            i32_type(),
            true,
        )])
    }

    fn run<F, T>(future: F) -> T
    where
        F: std::future::Future<Output = T> + 'static,
        T: 'static,
    {
        mudu_sys::task::async_::block_on_tokio_current_thread(future).unwrap()
    }

    #[test]
    fn filter_passes_only_matching_rows() {
        run(async {
            let rows = vec![
                TupleRow::new_nullable(vec![Some(i32_bin(1))]),
                TupleRow::new_nullable(vec![Some(i32_bin(3))]),
                TupleRow::new_nullable(vec![None]),
                TupleRow::new_nullable(vec![Some(i32_bin(5))]),
            ];
            let filter = FilterExec::new(
                desc(),
                exec_with_rows(rows),
                vec![ResidualFilter {
                    input_pos: 0,
                    data_type: i32_type(),
                    op: ValueCompare::GT,
                    literal: Some(i32_bin(2)),
                }],
                vec![0],
            );
            filter.open().await.unwrap();
            let mut values = Vec::new();
            while let Some(row) = filter.next().await.unwrap() {
                let value = TypedBin::new(TypeFamily::I32, row.fields()[0].clone().unwrap())
                    .to_value(&i32_type())
                    .unwrap();
                values.push(value.to_i32());
            }
            // NULL rows never pass a comparison filter.
            assert_eq!(values, vec![3, 5]);
        })
    }

    #[test]
    fn filter_supports_not_equal() {
        run(async {
            let rows = vec![
                TupleRow::new_nullable(vec![Some(i32_bin(1))]),
                TupleRow::new_nullable(vec![Some(i32_bin(2))]),
            ];
            let filter = FilterExec::new(
                desc(),
                exec_with_rows(rows),
                vec![ResidualFilter {
                    input_pos: 0,
                    data_type: i32_type(),
                    op: ValueCompare::NE,
                    literal: Some(i32_bin(1)),
                }],
                vec![0],
            );
            filter.open().await.unwrap();
            let row = filter.next().await.unwrap().unwrap();
            let value = TypedBin::new(TypeFamily::I32, row.fields()[0].clone().unwrap())
                .to_value(&i32_type())
                .unwrap();
            assert_eq!(value.to_i32(), 2);
            assert!(filter.next().await.unwrap().is_none());
        })
    }

    #[test]
    fn filter_null_literal_matches_nothing() {
        run(async {
            let rows = vec![TupleRow::new_nullable(vec![Some(i32_bin(1))])];
            let filter = FilterExec::new(
                desc(),
                exec_with_rows(rows),
                vec![ResidualFilter {
                    input_pos: 0,
                    data_type: i32_type(),
                    op: ValueCompare::EQ,
                    literal: None,
                }],
                vec![0],
            );
            filter.open().await.unwrap();
            assert!(filter.next().await.unwrap().is_none());
        })
    }

    #[test]
    fn filter_projects_and_reorders_columns() {
        run(async {
            let rows = vec![TupleRow::new_nullable(vec![
                Some(i32_bin(7)),
                Some(i32_bin(8)),
            ])];
            let filter = FilterExec::new(desc(), exec_with_rows(rows), vec![], vec![1]);
            filter.open().await.unwrap();
            let row = filter.next().await.unwrap().unwrap();
            assert_eq!(row.fields().len(), 1);
            let value = TypedBin::new(TypeFamily::I32, row.fields()[0].clone().unwrap())
                .to_value(&i32_type())
                .unwrap();
            assert_eq!(value.to_i32(), 8);
        })
    }
}
