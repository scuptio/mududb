use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::async_::futures_mutex::FMutex;
use std::sync::Arc;

use crate::contract::query_exec::QueryExec;

pub struct MuduResultSetAsync {
    desc: Arc<TupleFieldDesc>,
    inner: FMutex<ResultRows>,
}

struct ResultRows {
    rows: Vec<TupleValue>,
    index: usize,
}

impl MuduResultSetAsync {
    pub fn from_rows(rows: Vec<TupleValue>, desc: TupleFieldDesc) -> Self {
        Self {
            desc: Arc::new(desc),
            inner: FMutex::new(ResultRows { rows, index: 0 }),
        }
    }

    pub async fn from_query_exec(exec: Arc<dyn QueryExec>) -> RS<Self> {
        let (rows, desc) = super::mudu_conn_core::query_exec_to_rows(exec).await?;
        Ok(Self::from_rows(rows, desc))
    }
}

#[async_trait]
impl ResultSetAsync for MuduResultSetAsync {
    async fn next(&self) -> RS<Option<TupleValue>> {
        let mut inner = self.inner.lock().await;
        if inner.index >= inner.rows.len() {
            return Ok(None);
        }
        let index = inner.index;
        let row = inner.rows.remove(index);
        Ok(Some(row))
    }

    fn desc(&self) -> &TupleFieldDesc {
        self.desc.as_ref()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::contract::query_exec::QueryExec;
    use async_trait::async_trait;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_field::TupleField;
    use mudu_sys::sync::SMutex;
    use mudu_type::data_type::DataType;
    use mudu_type::data_value::DataValue;
    use mudu_type::type_family::TypeFamily;
    use std::collections::VecDeque;

    fn test_desc() -> TupleFieldDesc {
        TupleFieldDesc::new(vec![DatumDesc::new(
            "id".to_string(),
            DataType::default_for(TypeFamily::I32),
        )])
    }

    fn i32_bin(n: i32) -> Vec<u8> {
        n.to_be_bytes().to_vec()
    }

    struct TestQueryExec {
        rows: SMutex<VecDeque<TupleField>>,
        tuple_desc: TupleFieldDesc,
        open_error: bool,
    }

    #[async_trait]
    impl QueryExec for TestQueryExec {
        async fn open(&self) -> RS<()> {
            if self.open_error {
                Err(mudu_error!(ErrorCode::Internal, "open failed"))
            } else {
                Ok(())
            }
        }

        async fn next(&self) -> RS<Option<TupleField>> {
            Ok(self.rows.lock().unwrap().pop_front())
        }

        fn tuple_desc(&self) -> RS<TupleFieldDesc> {
            Ok(self.tuple_desc.clone())
        }
    }

    #[test]
    fn from_rows_next_returns_rows_in_order_then_none() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let rs = MuduResultSetAsync::from_rows(
                vec![
                    TupleValue::from(vec![DataValue::from_i32(1)]),
                    TupleValue::from(vec![DataValue::from_i32(2)]),
                ],
                test_desc(),
            );
            assert_eq!(
                rs.next().await.unwrap().unwrap().values()[0].as_i32(),
                Some(&1)
            );
            assert_eq!(
                rs.next().await.unwrap().unwrap().values()[0].as_i32(),
                Some(&2)
            );
            assert!(rs.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn from_rows_empty_first_next_is_none() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let rs = MuduResultSetAsync::from_rows(vec![], test_desc());
            assert!(rs.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn desc_returns_descriptor() {
        let desc = test_desc();
        let rs = MuduResultSetAsync::from_rows(vec![], desc.clone());
        assert_eq!(rs.desc().fields().len(), 1);
        assert_eq!(rs.desc().fields()[0].name(), "id");
    }

    #[test]
    fn from_rows_with_null_and_non_null_values() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let desc = TupleFieldDesc::new(vec![
                DatumDesc::new("a".to_string(), DataType::default_for(TypeFamily::I32)),
                DatumDesc::new("b".to_string(), DataType::default_for(TypeFamily::I32)),
            ]);
            let rs = MuduResultSetAsync::from_rows(
                vec![TupleValue::from(vec![
                    DataValue::from_i32(1),
                    DataValue::null(),
                ])],
                desc,
            );
            let row = rs.next().await.unwrap().unwrap();
            let values = row.values();
            assert_eq!(values[0].as_i32(), Some(&1));
            assert!(values[1].is_null());
        })
        .unwrap()
    }

    #[test]
    fn from_query_exec_with_rows() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let exec: Arc<dyn QueryExec> = Arc::new(TestQueryExec {
                rows: SMutex::new(VecDeque::from(vec![
                    TupleField::new(vec![i32_bin(10)]),
                    TupleField::new(vec![i32_bin(20)]),
                ])),
                tuple_desc: test_desc(),
                open_error: false,
            });
            let rs = MuduResultSetAsync::from_query_exec(exec).await.unwrap();
            assert_eq!(
                rs.next().await.unwrap().unwrap().values()[0].as_i32(),
                Some(&10)
            );
            assert_eq!(
                rs.next().await.unwrap().unwrap().values()[0].as_i32(),
                Some(&20)
            );
            assert!(rs.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn from_query_exec_open_failure_propagates() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let exec: Arc<dyn QueryExec> = Arc::new(TestQueryExec {
                rows: SMutex::new(VecDeque::new()),
                tuple_desc: test_desc(),
                open_error: true,
            });
            let err = match MuduResultSetAsync::from_query_exec(exec).await {
                Ok(_) => panic!("expected error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::Internal);
        })
        .unwrap()
    }
}
