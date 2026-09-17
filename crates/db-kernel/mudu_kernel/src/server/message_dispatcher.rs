use std::sync::OnceLock;

use crate::server::async_func_task::HandleResult;
use crate::server::handlers::{
    BatchHandler, ExecuteHandler, GetHandler, HandshakeHandler, ProcedureInvokeHandler, PutHandler,
    QueryHandler, RangeScanHandler, SessionCloseHandler, SessionCreateHandler,
};
use crate::server::request_ctx::RequestCtx;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{Frame, MessageType};
use mudu_sys::scoped_task_trace;

#[async_trait]
pub(in crate::server) trait MessageHandler: Send + Sync {
    fn message_type(&self) -> MessageType;
    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult>;
}

pub(in crate::server) struct MessageDispatcher {
    handlers: Vec<(MessageType, Box<dyn MessageHandler>)>,
}

impl MessageDispatcher {
    pub(in crate::server) fn global() -> &'static Self {
        static INSTANCE: OnceLock<MessageDispatcher> = OnceLock::new();
        INSTANCE.get_or_init(Self::new)
    }

    fn new() -> Self {
        let mut handlers: Vec<(MessageType, Box<dyn MessageHandler>)> = Vec::new();
        register(&mut handlers, Box::new(HandshakeHandler));
        register(&mut handlers, Box::new(QueryHandler));
        register(&mut handlers, Box::new(ExecuteHandler));
        register(&mut handlers, Box::new(BatchHandler));
        register(&mut handlers, Box::new(GetHandler));
        register(&mut handlers, Box::new(PutHandler));
        register(&mut handlers, Box::new(RangeScanHandler));
        register(&mut handlers, Box::new(ProcedureInvokeHandler));
        register(&mut handlers, Box::new(SessionCreateHandler));
        register(&mut handlers, Box::new(SessionCloseHandler));
        Self { handlers }
    }

    pub(in crate::server) async fn dispatch(
        &self,
        ctx: &RequestCtx,
        frame: &Frame,
    ) -> Option<RS<HandleResult>> {
        scoped_task_trace!();
        let (_, handler) = self
            .handlers
            .iter()
            .find(|(message_type, _)| *message_type == frame.header().message_type())?;
        Some(handler.handle(ctx, frame).await)
    }
}

fn register(
    handlers: &mut Vec<(MessageType, Box<dyn MessageHandler>)>,
    handler: Box<dyn MessageHandler>,
) {
    handlers.push((handler.message_type(), handler));
}
