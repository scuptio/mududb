use std::sync::OnceLock;

use crate::async_func_task::HandleResult;
use crate::handlers::{
    BatchHandler, ExecuteHandler, GetHandler, HandshakeHandler, MessageHandler,
    ProcedureInvokeHandler, PutHandler, QueryHandler, RangeScanHandler, SessionCloseHandler,
    SessionCreateHandler,
};
use crate::request_ctx::RequestCtx;
use mudu::common::result::RS;
use mudu_contract::protocol::{Frame, MessageType};
use mudu_sys::scoped_task_trace;

pub(crate) struct MessageDispatcher {
    handlers: Vec<(MessageType, Box<dyn MessageHandler>)>,
}

impl MessageDispatcher {
    pub(crate) fn global() -> &'static Self {
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

    pub(crate) async fn dispatch(
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
