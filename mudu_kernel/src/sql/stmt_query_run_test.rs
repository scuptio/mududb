#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::contract::query_exec::QueryExec;
    use crate::contract::ssn_ctx::SsnCtx;
    use crate::sql::proj_field::ProjField;
    use crate::sql::proj_list::ProjList;
    use crate::sql::stmt_query::StmtQuery;
    use crate::sql::stmt_query_run::run_query_stmt;
    use async_trait::async_trait;
    use futures::StreamExt;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_field::TupleField;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_sys::sync::SMutex;
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;
    use pgwire::api::Type as PGDataType;
    use pgwire::error::PgWireError;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct TestSsnCtx {
        current_tx: SMutex<Option<OID>>,
        ended: AtomicBool,
    }

    impl TestSsnCtx {
        fn ended(&self) -> bool {
            self.ended.load(Ordering::SeqCst)
        }
    }

    impl SsnCtx for TestSsnCtx {
        fn current_tx(&self) -> Option<OID> {
            *self.current_tx.lock().unwrap()
        }

        fn begin_tx(&self, xid: OID) -> RS<()> {
            *self.current_tx.lock().unwrap() = Some(xid);
            Ok(())
        }

        fn end_tx(&self) -> RS<()> {
            self.ended.store(true, Ordering::SeqCst);
            Ok(())
        }
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

    struct TestStmtQuery {
        fail_realize: bool,
        fail_build: bool,
        exec: Arc<dyn QueryExec>,
        proj_list: ProjList,
    }

    #[async_trait]
    impl StmtQuery for TestStmtQuery {
        async fn realize(&self, _ctx: &dyn SsnCtx) -> RS<()> {
            if self.fail_realize {
                Err(mudu_error!(ErrorCode::Internal, "realize failed"))
            } else {
                Ok(())
            }
        }

        async fn build(&self, _ctx: &dyn SsnCtx) -> RS<Arc<dyn QueryExec>> {
            if self.fail_build {
                Err(mudu_error!(ErrorCode::Internal, "build failed"))
            } else {
                Ok(self.exec.clone())
            }
        }

        fn proj_list(&self) -> RS<ProjList> {
            Ok(self.proj_list.clone())
        }
    }

    fn int_proj_list() -> ProjList {
        ProjList::new(vec![ProjField::new(
            0,
            "id".to_string(),
            DataType::default_for(TypeFamily::I32),
        )])
    }

    fn int_tuple_desc() -> TupleFieldDesc {
        TupleFieldDesc::new(vec![DatumDesc::new(
            "id".to_string(),
            DataType::default_for(TypeFamily::I32),
        )])
    }

    fn query_exec_with_rows(rows: Vec<TupleField>) -> Arc<dyn QueryExec> {
        Arc::new(TestQueryExec {
            rows: SMutex::new(VecDeque::from(rows)),
            tuple_desc: int_tuple_desc(),
            open_error: false,
        })
    }

    fn query_exec_with_open_error() -> Arc<dyn QueryExec> {
        Arc::new(TestQueryExec {
            rows: SMutex::new(VecDeque::new()),
            tuple_desc: int_tuple_desc(),
            open_error: true,
        })
    }

    fn i32_field(n: i32) -> Vec<u8> {
        n.to_be_bytes().to_vec()
    }

    fn i128_field(n: i128) -> Vec<u8> {
        n.to_be_bytes().to_vec()
    }

    #[test]
    fn run_query_stmt_returns_stream_on_success() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: query_exec_with_rows(vec![]),
                proj_list: int_proj_list(),
            };

            let (fields, mut stream) = run_query_stmt(&stmt, &ctx).await.unwrap();
            assert_eq!(fields.len(), 1);
            assert!(stream.next().await.is_none());
            assert!(ctx.current_tx().is_some());
            assert!(!ctx.ended());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_realize_failure_is_propagated_and_ends_tx() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let stmt = TestStmtQuery {
                fail_realize: true,
                fail_build: false,
                exec: query_exec_with_rows(vec![]),
                proj_list: int_proj_list(),
            };

            let err = match run_query_stmt(&stmt, &ctx).await {
                Ok(_) => panic!("expected error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::Internal);
            assert!(err.to_string().contains("realize failed"));
            assert!(ctx.ended());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_build_failure_is_propagated() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: true,
                exec: query_exec_with_rows(vec![]),
                proj_list: int_proj_list(),
            };

            let err = match run_query_stmt(&stmt, &ctx).await {
                Ok(_) => panic!("expected error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::Internal);
            assert!(err.to_string().contains("build failed"));
            assert!(ctx.ended());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_open_failure_is_propagated() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: query_exec_with_open_error(),
                proj_list: int_proj_list(),
            };

            let err = match run_query_stmt(&stmt, &ctx).await {
                Ok(_) => panic!("expected error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::Internal);
            assert!(err.to_string().contains("open failed"));
            assert!(ctx.ended());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_single_i32_row() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: query_exec_with_rows(vec![TupleField::new(vec![i32_field(42)])]),
                proj_list: int_proj_list(),
            };

            let (fields, mut stream) = run_query_stmt(&stmt, &ctx).await.unwrap();
            assert_eq!(fields.len(), 1);
            assert_eq!(*fields[0].datatype(), PGDataType::INT4);

            let row = stream.next().await.unwrap().unwrap();
            assert_eq!(row.field_count, 1);
            assert!(row.data.len() >= 6);
            assert!(stream.next().await.is_none());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_multiple_rows_in_order() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: query_exec_with_rows(vec![
                    TupleField::new(vec![i32_field(10)]),
                    TupleField::new(vec![i32_field(20)]),
                    TupleField::new(vec![i32_field(30)]),
                ]),
                proj_list: int_proj_list(),
            };

            let (_, mut stream) = run_query_stmt(&stmt, &ctx).await.unwrap();
            let first = stream.next().await.unwrap().unwrap();
            let second = stream.next().await.unwrap().unwrap();
            let third = stream.next().await.unwrap().unwrap();
            assert!(stream.next().await.is_none());

            assert_eq!(first.field_count, 1);
            assert_eq!(second.field_count, 1);
            assert_eq!(third.field_count, 1);
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_null_field_encoded_as_null() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: query_exec_with_rows(vec![TupleField::new_nullable(vec![None])]),
                proj_list: int_proj_list(),
            };

            let (_, mut stream) = run_query_stmt(&stmt, &ctx).await.unwrap();
            let row = stream.next().await.unwrap().unwrap();
            assert_eq!(row.field_count, 1);
            assert_eq!(row.data.len(), 4);
            assert_eq!(&row.data[..], &[0xff, 0xff, 0xff, 0xff]);
            assert!(stream.next().await.is_none());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_mixed_null_non_null_bitmap() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let tuple_desc = TupleFieldDesc::new(vec![
                DatumDesc::new("a".to_string(), DataType::default_for(TypeFamily::I32)),
                DatumDesc::new("b".to_string(), DataType::default_for(TypeFamily::I32)),
                DatumDesc::new("c".to_string(), DataType::default_for(TypeFamily::I32)),
            ]);
            let proj_list = ProjList::new(vec![
                ProjField::new(0, "a".to_string(), DataType::default_for(TypeFamily::I32)),
                ProjField::new(1, "b".to_string(), DataType::default_for(TypeFamily::I32)),
                ProjField::new(2, "c".to_string(), DataType::default_for(TypeFamily::I32)),
            ]);
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: Arc::new(TestQueryExec {
                    rows: SMutex::new(VecDeque::from(vec![TupleField::new_nullable(vec![
                        Some(i32_field(1)),
                        None,
                        Some(i32_field(3)),
                    ])])),
                    tuple_desc,
                    open_error: false,
                }),
                proj_list,
            };

            let (_, mut stream) = run_query_stmt(&stmt, &ctx).await.unwrap();
            let row = stream.next().await.unwrap().unwrap();
            assert_eq!(row.field_count, 3);
            assert!(stream.next().await.is_none());
        })
        .unwrap()
    }

    #[test]
    fn to_pg_field_info_unsupported_type_returns_invalid_type() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let proj_list = ProjList::new(vec![ProjField::new(
                0,
                "x".to_string(),
                DataType::default_for(TypeFamily::I128),
            )]);
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: query_exec_with_rows(vec![]),
                proj_list,
            };

            let err = match run_query_stmt(&stmt, &ctx).await {
                Ok(_) => panic!("expected error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::InvalidType);
            assert!(ctx.ended());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_tuple_desc_field_count_mismatch_returns_fatal_internal() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let tuple_desc = TupleFieldDesc::new(vec![
                DatumDesc::new("a".to_string(), DataType::default_for(TypeFamily::I32)),
                DatumDesc::new("b".to_string(), DataType::default_for(TypeFamily::I32)),
            ]);
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: Arc::new(TestQueryExec {
                    rows: SMutex::new(VecDeque::from(vec![TupleField::new(vec![i32_field(1)])])),
                    tuple_desc,
                    open_error: false,
                }),
                proj_list: int_proj_list(),
            };

            let err = match run_query_stmt(&stmt, &ctx).await {
                Ok(_) => panic!("expected error"),
                Err(e) => e,
            };
            assert!(err
                .to_string()
                .contains("fatal error: non consistent column number"));
            assert!(ctx.ended());
        })
        .unwrap()
    }

    #[test]
    fn run_query_stmt_unsupported_datum_type_yields_pgwire_error_in_stream() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let ctx = TestSsnCtx::default();
            let tuple_desc = TupleFieldDesc::new(vec![DatumDesc::new(
                "x".to_string(),
                DataType::default_for(TypeFamily::I128),
            )]);
            let stmt = TestStmtQuery {
                fail_realize: false,
                fail_build: false,
                exec: Arc::new(TestQueryExec {
                    rows: SMutex::new(VecDeque::from(vec![TupleField::new(vec![i128_field(42)])])),
                    tuple_desc,
                    open_error: false,
                }),
                proj_list: int_proj_list(),
            };

            let (_, mut stream) = run_query_stmt(&stmt, &ctx).await.unwrap();
            let item = stream.next().await.unwrap();
            assert!(matches!(item, Err(PgWireError::ApiError(_))));
        })
        .unwrap()
    }
}
