#[cfg(test)]
mod tests {
    use crate::contract::cmd_exec::CmdExec;
    use crate::contract::ssn_ctx::SsnCtx;
    use crate::sql::stmt_cmd::StmtCmd;
    use crate::sql::stmt_cmd_run::run_cmd_stmt;
    use async_trait::async_trait;
    use mudu::common::result::RS;
    use mudu::common::id::OID;
    use mudu::error::ec::EC;
    use mudu::m_error;
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

    struct TestCmdExec {
        fail_prepare: bool,
        fail_run: bool,
        affected_rows: u64,
    }

    #[async_trait]
    impl CmdExec for TestCmdExec {
        async fn prepare(&self) -> RS<()> {
            if self.fail_prepare {
                Err(m_error!(EC::InternalErr, "prepare failed"))
            } else {
                Ok(())
            }
        }

        async fn run(&self) -> RS<()> {
            if self.fail_run {
                Err(m_error!(EC::InternalErr, "run failed"))
            } else {
                Ok(())
            }
        }

        async fn affected_rows(&self) -> RS<u64> {
            Ok(self.affected_rows)
        }
    }

    struct TestStmtCmd {
        fail_realize: bool,
        fail_build: bool,
        exec: Arc<dyn CmdExec>,
    }

    #[async_trait]
    impl StmtCmd for TestStmtCmd {
        async fn realize(&self, _ctx: &dyn SsnCtx) -> RS<()> {
            if self.fail_realize {
                Err(m_error!(EC::InternalErr, "realize failed"))
            } else {
                Ok(())
            }
        }

        async fn build(&self, _ctx: &dyn SsnCtx) -> RS<Arc<dyn CmdExec>> {
            if self.fail_build {
                Err(m_error!(EC::InternalErr, "build failed"))
            } else {
                Ok(self.exec.clone())
            }
        }
    }

    #[test]
    fn run_cmd_stmt_returns_affected_rows_on_success() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let ctx = TestSsnCtx::default();
        let stmt = TestStmtCmd {
            fail_realize: false,
            fail_build: false,
            exec: Arc::new(TestCmdExec {
                fail_prepare: false,
                fail_run: false,
                affected_rows: 3,
            }),
        };

        let rows = run_cmd_stmt(&stmt, &ctx).await.unwrap();
        assert_eq!(rows, 3);
        assert!(ctx.current_tx().is_some());
        assert!(!ctx.ended());
        }).unwrap()
    }

    #[test]
    fn run_cmd_stmt_ends_tx_on_build_error() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let ctx = TestSsnCtx::default();
        let stmt = TestStmtCmd {
            fail_realize: false,
            fail_build: true,
            exec: Arc::new(TestCmdExec {
                fail_prepare: false,
                fail_run: false,
                affected_rows: 0,
            }),
        };

        let err = run_cmd_stmt(&stmt, &ctx).await.unwrap_err();
        assert!(err.to_string().contains("build failed"));
        assert!(ctx.ended());
        }).unwrap()
    }
}
