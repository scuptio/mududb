use crate::server::session::{DummyAuthSource, Session};
use crate::x_engine::thd_ctx::ThdCtx;
use pgwire::api::auth::md5pass::Md5PasswordAuthStartupHandler;
use pgwire::api::auth::DefaultServerParameterProvider;
use pgwire::api::copy::NoopCopyHandler;
use pgwire::api::{NoopErrorHandler, PgWireServerHandlers};
use std::sync::Arc;

#[derive(Clone)]
pub struct SessionMgr {
    ctx: ThdCtx,
}

impl SessionMgr {
    pub fn new(ctx: ThdCtx) -> Self {
        Self { ctx }
    }
}

impl PgWireServerHandlers for SessionMgr {
    type StartupHandler =
    Md5PasswordAuthStartupHandler<DummyAuthSource, DefaultServerParameterProvider>;
    type SimpleQueryHandler = Session;
    type ExtendedQueryHandler = Session;
    type CopyHandler = NoopCopyHandler;
    type ErrorHandler = NoopErrorHandler;

    fn simple_query_handler(&self) -> Arc<Self::SimpleQueryHandler> {
        Arc::new(Session::new(self.ctx.clone()))
    }

    fn extended_query_handler(&self) -> Arc<Self::ExtendedQueryHandler> {
        Arc::new(Session::new(self.ctx.clone()))
    }

    fn startup_handler(&self) -> Arc<Self::StartupHandler> {
        Arc::new(Md5PasswordAuthStartupHandler::new(
            Arc::new(DummyAuthSource),
            Arc::new(DefaultServerParameterProvider::default()),
        ))
    }

    fn copy_handler(&self) -> Arc<Self::CopyHandler> {
        Arc::new(NoopCopyHandler)
    }

    fn error_handler(&self) -> Arc<Self::ErrorHandler> {
        Arc::new(NoopErrorHandler)
    }
}
