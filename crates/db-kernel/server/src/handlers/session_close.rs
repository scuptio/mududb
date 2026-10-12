use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{decode_session_close_request, Frame, MessageType};

use super::MessageHandler;
use crate::async_func_task::HandleResult;
use crate::request_ctx::RequestCtx;

pub(crate) struct SessionCloseHandler;

#[async_trait]
impl MessageHandler for SessionCloseHandler {
    fn message_type(&self) -> MessageType {
        MessageType::SessionClose
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let request = decode_session_close_request(frame)?;
        ctx.session_close(request.session_id()).await
    }
}
