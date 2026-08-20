//! Whole-set aggregate executor (no `GROUP BY`).
//!
//! Wraps a child executor, drains it on `open()`, accumulates the
//! aggregates, and emits exactly one result row. NULL inputs are skipped
//! (except by `COUNT(*)`); on an empty input set COUNT yields 0 and every
//! other aggregate yields NULL.

use crate::contract::query_exec::QueryExec;
use crate::executor::value_compare::compare_values;
use crate::sql::bound_stmt::AggregateFunc;
use crate::x_engine::api::TupleRow;
use async_trait::async_trait;
use bigdecimal::{BigDecimal, ToPrimitive};
use mudu::common::result::RS;
use mudu::data_type::numeric::Numeric;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::typed_bin::TypedBin;
use mudu_sys::sync::async_::futures_mutex::FMutex;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use std::cmp::Ordering;
use std::sync::Arc;

/// An aggregate call resolved against the child executor's row layout.
#[derive(Clone)]
pub struct AggregateSpec {
    pub func: AggregateFunc,
    /// Position of the argument column in the child row; `None` means
    /// `COUNT(*)`.
    pub arg_pos: Option<usize>,
    /// Type of the argument column (used to decode it); `None` for
    /// `COUNT(*)`.
    pub arg_type: Option<DataType>,
    /// Type of the aggregate result.
    pub result_type: DataType,
}

enum Accumulator {
    Count(i64),
    /// SUM/AVG over exact numeric families (I32/I64/NUMERIC).
    SumDecimal {
        sum: BigDecimal,
        count: i64,
    },
    /// SUM/AVG over floating point families (F32/F64).
    SumFloat {
        sum: f64,
        count: i64,
    },
    MinMax {
        best: Option<DataValue>,
        is_min: bool,
    },
}

impl Accumulator {
    fn new(spec: &AggregateSpec) -> RS<Self> {
        let acc = match spec.func {
            AggregateFunc::Count => Accumulator::Count(0),
            AggregateFunc::Sum | AggregateFunc::Avg => {
                let family = spec
                    .arg_type
                    .as_ref()
                    .ok_or_else(|| {
                        mudu_error!(ER::InvalidState, "sum/avg requires an argument type")
                    })?
                    .type_family();
                match family {
                    TypeFamily::I32 | TypeFamily::I64 | TypeFamily::Numeric => {
                        Accumulator::SumDecimal {
                            sum: BigDecimal::from(0),
                            count: 0,
                        }
                    }
                    TypeFamily::F32 | TypeFamily::F64 => {
                        Accumulator::SumFloat { sum: 0.0, count: 0 }
                    }
                    _ => {
                        return Err(mudu_error!(
                            ER::NotImplemented,
                            format!("aggregate argument type {:?} is not numeric", family)
                        ))
                    }
                }
            }
            AggregateFunc::Min => Accumulator::MinMax {
                best: None,
                is_min: true,
            },
            AggregateFunc::Max => Accumulator::MinMax {
                best: None,
                is_min: false,
            },
        };
        Ok(acc)
    }

    fn feed(&mut self, value: Option<DataValue>) -> RS<()> {
        match self {
            Accumulator::Count(n) => {
                // COUNT(*) passes a placeholder non-null value per row;
                // COUNT(col) only counts non-NULL values.
                if value.is_some() {
                    *n += 1;
                }
            }
            Accumulator::SumDecimal { sum, count } => {
                let Some(value) = value else {
                    return Ok(());
                };
                let increment = if let Some(v) = value.as_i32() {
                    BigDecimal::from(*v)
                } else if let Some(v) = value.as_i64() {
                    BigDecimal::from(*v)
                } else if let Some(v) = value.as_numeric() {
                    v.as_bigdecimal().clone()
                } else {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "SUM/AVG argument is not an exact numeric"
                    ));
                };
                *sum += increment;
                *count += 1;
            }
            Accumulator::SumFloat { sum, count } => {
                let Some(value) = value else {
                    return Ok(());
                };
                if let Some(v) = value.as_f32() {
                    *sum += *v as f64;
                } else if let Some(v) = value.as_f64() {
                    *sum += *v;
                } else {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "SUM/AVG argument is not a floating point number"
                    ));
                }
                *count += 1;
            }
            Accumulator::MinMax { best, is_min } => {
                let Some(value) = value else {
                    return Ok(());
                };
                let replace = match best {
                    None => true,
                    Some(current) => {
                        let ordering = compare_values(&value, current)?;
                        match ordering {
                            Some(Ordering::Less) => *is_min,
                            Some(Ordering::Greater) => !*is_min,
                            _ => false,
                        }
                    }
                };
                if replace {
                    *best = Some(value);
                }
            }
        }
        Ok(())
    }

    fn finish(self, spec: &AggregateSpec) -> RS<DataValue> {
        let value = match self {
            Accumulator::Count(n) => DataValue::from_i64(n),
            Accumulator::SumDecimal { sum, count } => {
                if count == 0 {
                    return Ok(DataValue::null());
                }
                match spec.func {
                    AggregateFunc::Sum => match spec.result_type.type_family() {
                        TypeFamily::I64 => {
                            let n = sum.to_i64().ok_or_else(|| {
                                mudu_error!(ER::InvalidArgument, "SUM result overflows i64")
                            })?;
                            DataValue::from_i64(n)
                        }
                        TypeFamily::Numeric => {
                            DataValue::from_numeric(Numeric::from_bigdecimal(sum))
                        }
                        family => {
                            return Err(mudu_error!(
                                ER::InvalidState,
                                format!("unexpected SUM result type {:?}", family)
                            ))
                        }
                    },
                    // AVG over exact numerics yields NUMERIC.
                    _ => DataValue::from_numeric(Numeric::from_bigdecimal(
                        sum / BigDecimal::from(count),
                    )),
                }
            }
            Accumulator::SumFloat { sum, count } => {
                if count == 0 {
                    return Ok(DataValue::null());
                }
                match spec.func {
                    AggregateFunc::Sum => DataValue::from_f64(sum),
                    _ => DataValue::from_f64(sum / count as f64),
                }
            }
            Accumulator::MinMax { best, .. } => match best {
                Some(value) => value,
                None => return Ok(DataValue::null()),
            },
        };
        Ok(value)
    }
}

pub struct AggregateExec {
    tuple_desc: TupleFieldDesc,
    child: Arc<dyn QueryExec>,
    specs: Vec<AggregateSpec>,
    inner: FMutex<AggregateInner>,
}

struct AggregateInner {
    result: Option<TupleRow>,
    emitted: bool,
}

impl AggregateExec {
    pub fn new(
        tuple_desc: TupleFieldDesc,
        child: Arc<dyn QueryExec>,
        specs: Vec<AggregateSpec>,
    ) -> Self {
        Self {
            tuple_desc,
            child,
            specs,
            inner: FMutex::new(AggregateInner {
                result: None,
                emitted: false,
            }),
        }
    }

    fn decode_arg(spec: &AggregateSpec, row: &TupleRow) -> RS<Option<DataValue>> {
        let arg_pos = spec
            .arg_pos
            .ok_or_else(|| mudu_error!(ER::InvalidState, "aggregate requires an argument"))?;
        let arg_type = spec
            .arg_type
            .as_ref()
            .ok_or_else(|| mudu_error!(ER::InvalidState, "aggregate requires a type"))?;
        let field = row
            .fields()
            .get(arg_pos)
            .ok_or_else(|| mudu_error!(ER::InvalidState, "aggregate argument out of row bounds"))?;
        match field {
            Some(binary) => Ok(Some(
                TypedBin::new(arg_type.type_family(), binary.clone()).to_value(arg_type)?,
            )),
            None => Ok(None),
        }
    }
}

#[async_trait]
impl QueryExec for AggregateExec {
    async fn open(&self) -> RS<()> {
        self.child.open().await?;

        let mut accumulators = self
            .specs
            .iter()
            .map(Accumulator::new)
            .collect::<RS<Vec<_>>>()?;
        while let Some(row) = self.child.next().await? {
            for (spec, acc) in self.specs.iter().zip(accumulators.iter_mut()) {
                let value = match spec.arg_pos {
                    // COUNT(*) counts every row; feed a placeholder.
                    None => Some(DataValue::from_i64(1)),
                    Some(_) => Self::decode_arg(spec, &row)?,
                };
                acc.feed(value)?;
            }
        }

        let mut fields = Vec::with_capacity(self.specs.len());
        for (spec, acc) in self.specs.iter().zip(accumulators) {
            let value = acc.finish(spec)?;
            if value.is_null() {
                fields.push(None);
            } else {
                let binary = value.to_binary(&spec.result_type)?;
                fields.push(Some(binary.into()));
            }
        }

        let mut inner = self.inner.lock().await;
        inner.result = Some(TupleRow::new_nullable(fields));
        Ok(())
    }

    async fn next(&self) -> RS<Option<TupleRow>> {
        let mut inner = self.inner.lock().await;
        if inner.emitted {
            return Ok(None);
        }
        inner.emitted = true;
        Ok(inner.result.clone())
    }

    fn tuple_desc(&self) -> RS<TupleFieldDesc> {
        Ok(self.tuple_desc.clone())
    }
}

unsafe impl Send for AggregateExec {}

unsafe impl Sync for AggregateExec {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_sys::sync::SMutex;
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

    fn i64_type() -> DataType {
        DataType::default_for(TypeFamily::I64)
    }

    fn numeric_type() -> DataType {
        DataType::from_numeric(mudu_type::data_type_param_numeric::DataTypeParamNumeric::new(38, 6))
    }

    fn i32_bin(value: i32) -> Vec<u8> {
        DataValue::from_i32(value)
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
            "agg".to_string(),
            i64_type(),
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

    fn decode(row: &TupleRow, pos: usize, ty: &DataType) -> DataValue {
        TypedBin::new(ty.type_family(), row.fields()[pos].clone().unwrap())
            .to_value(ty)
            .unwrap()
    }

    fn spec(func: AggregateFunc, arg_pos: Option<usize>, result_type: DataType) -> AggregateSpec {
        AggregateSpec {
            func,
            arg_pos,
            arg_type: arg_pos.map(|_| i32_type()),
            result_type,
        }
    }

    fn run_aggregate(
        rows: Vec<TupleRow>,
        specs: Vec<AggregateSpec>,
    ) -> (Option<TupleRow>, Option<TupleRow>) {
        run(async {
            let agg = AggregateExec::new(desc(), exec_with_rows(rows), specs);
            agg.open().await.unwrap();
            let first = agg.next().await.unwrap();
            let second = agg.next().await.unwrap();
            (first, second)
        })
    }

    #[test]
    fn count_star_counts_all_rows() {
        let rows = vec![
            TupleRow::new_nullable(vec![Some(i32_bin(1))]),
            TupleRow::new_nullable(vec![None]),
            TupleRow::new_nullable(vec![Some(i32_bin(3))]),
        ];
        let (first, second) =
            run_aggregate(rows, vec![spec(AggregateFunc::Count, None, i64_type())]);
        let row = first.unwrap();
        assert_eq!(decode(&row, 0, &i64_type()).to_i64(), 3);
        assert!(second.is_none());
    }

    #[test]
    fn count_column_skips_null() {
        let rows = vec![
            TupleRow::new_nullable(vec![Some(i32_bin(1))]),
            TupleRow::new_nullable(vec![None]),
        ];
        let (first, _) = run_aggregate(rows, vec![spec(AggregateFunc::Count, Some(0), i64_type())]);
        assert_eq!(decode(&first.unwrap(), 0, &i64_type()).to_i64(), 1);
    }

    #[test]
    fn count_star_over_empty_input_yields_zero() {
        let (first, _) = run_aggregate(vec![], vec![spec(AggregateFunc::Count, None, i64_type())]);
        assert_eq!(decode(&first.unwrap(), 0, &i64_type()).to_i64(), 0);
    }

    #[test]
    fn sum_int_yields_i64() {
        let rows = vec![
            TupleRow::new_nullable(vec![Some(i32_bin(10))]),
            TupleRow::new_nullable(vec![Some(i32_bin(20))]),
            TupleRow::new_nullable(vec![None]),
        ];
        let (first, _) = run_aggregate(rows, vec![spec(AggregateFunc::Sum, Some(0), i64_type())]);
        assert_eq!(decode(&first.unwrap(), 0, &i64_type()).to_i64(), 30);
    }

    #[test]
    fn sum_over_empty_input_yields_null() {
        let (first, _) = run_aggregate(vec![], vec![spec(AggregateFunc::Sum, Some(0), i64_type())]);
        let row = first.unwrap();
        assert!(row.fields()[0].is_none());
    }

    #[test]
    fn avg_int_yields_numeric() {
        let rows = vec![
            TupleRow::new_nullable(vec![Some(i32_bin(2))]),
            TupleRow::new_nullable(vec![Some(i32_bin(3))]),
        ];
        let (first, _) = run_aggregate(
            rows,
            vec![spec(AggregateFunc::Avg, Some(0), numeric_type())],
        );
        let value = decode(&first.unwrap(), 0, &numeric_type());
        assert_eq!(value.as_numeric().unwrap().to_plain_string(), "2.500000");
    }

    #[test]
    fn min_max_skip_null() {
        let rows = vec![
            TupleRow::new_nullable(vec![None]),
            TupleRow::new_nullable(vec![Some(i32_bin(7))]),
            TupleRow::new_nullable(vec![Some(i32_bin(4))]),
        ];
        let (min_row, _) = run_aggregate(
            rows.clone(),
            vec![spec(AggregateFunc::Min, Some(0), i32_type())],
        );
        assert_eq!(decode(&min_row.unwrap(), 0, &i32_type()).to_i32(), 4);
        let (max_row, _) = run_aggregate(rows, vec![spec(AggregateFunc::Max, Some(0), i32_type())]);
        assert_eq!(decode(&max_row.unwrap(), 0, &i32_type()).to_i32(), 7);
    }

    #[test]
    fn multiple_aggregates_share_one_scan() {
        let rows = vec![
            TupleRow::new_nullable(vec![Some(i32_bin(5))]),
            TupleRow::new_nullable(vec![Some(i32_bin(15))]),
        ];
        let (first, _) = run_aggregate(
            rows,
            vec![
                spec(AggregateFunc::Count, None, i64_type()),
                spec(AggregateFunc::Sum, Some(0), i64_type()),
            ],
        );
        let row = first.unwrap();
        assert_eq!(decode(&row, 0, &i64_type()).to_i64(), 2);
        assert_eq!(decode(&row, 1, &i64_type()).to_i64(), 20);
    }
}
