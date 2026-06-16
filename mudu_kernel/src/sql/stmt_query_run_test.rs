#[cfg(test)]
mod tests {
    use crate::contract::query_exec::QueryExec;
    use crate::contract::ssn_ctx::SsnCtx;
    use crate::sql::proj_field::ProjField;
    use crate::sql::proj_list::ProjList;
    use crate::sql::stmt_query::StmtQuery;
    use crate::sql::stmt_query_run::run_query_stmt;
    use async_trait::async_trait;
    use futures::StreamExt;
    use mudu_utils::oid::gen_oid;
    use mudu::common::result::RS;
    use mudu::common::id::OID;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_field::TupleField;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
use mudu_sys::sync::SMutex;

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
    }

    #[async_trait]
    impl QueryExec for TestQueryExec {
        async fn open(&self) -> RS<()> {
            Ok(())
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
        exec: Arc<dyn QueryExec>,
        proj_list: ProjList,
    }

    #[async_trait]
    impl StmtQuery for TestStmtQuery {
        async fn realize(&self, _ctx: &dyn SsnCtx) -> RS<()> {
            if self.fail_realize {
                Err(m_error!(EC::InternalErr, "realize failed"))
            } else {
                Ok(())
            }
        }

        async fn build(&self, _ctx: &dyn SsnCtx) -> RS<Arc<dyn QueryExec>> {
            Ok(self.exec.clone())
        }

        fn proj_list(&self) -> RS<ProjList> {
            Ok(self.proj_list.clone())
        }
    }

    fn int_proj_list() -> ProjList {
        ProjList::new(vec![ProjField::new(
            0,
            gen_oid(),
            "id".to_string(),
            DatType::default_for(DatTypeID::I32),
        )])
    }

    fn int_tuple_desc() -> TupleFieldDesc {
        TupleFieldDesc::new(vec![DatumDesc::new(
            "id".to_string(),
            DatType::default_for(DatTypeID::I32),
        )])
    }

    #[test]
    fn run_query_stmt_returns_stream_on_success() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let ctx = TestSsnCtx::default();
        let stmt = TestStmtQuery {
            fail_realize: false,
            exec: Arc::new(TestQueryExec {
                rows: SMutex::new(VecDeque::new()),
                tuple_desc: int_tuple_desc(),
            }),
            proj_list: int_proj_list(),
        };

        let (fields, mut stream) = run_query_stmt(&stmt, &ctx).await.unwrap();
        assert_eq!(fields.len(), 1);
        assert!(stream.next().await.is_none());
        assert!(ctx.current_tx().is_some());
        assert!(!ctx.ended());
        }).unwrap()
    }

    #[test]
    fn run_query_stmt_ends_tx_on_row_shape_error() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let ctx = TestSsnCtx::default();
        let stmt = TestStmtQuery {
            fail_realize: false,
            exec: Arc::new(TestQueryExec {
                rows: SMutex::new(VecDeque::from(vec![TupleField::new(vec![])])),
                tuple_desc: int_tuple_desc(),
            }),
            proj_list: int_proj_list(),
        };

        let err = match run_query_stmt(&stmt, &ctx).await {
            Ok(_) => panic!("expected query error"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("fatal error: non consistent column number"));
        assert!(ctx.ended());
        }).unwrap()
    }
}
