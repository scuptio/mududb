use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{decode_get_request, Frame, MessageType};

use crate::server::async_func_task::HandleResult;
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;

pub(in crate::server) struct GetHandler;

#[async_trait]
impl MessageHandler for GetHandler {
    fn message_type(&self) -> MessageType {
        MessageType::Get
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let request = decode_get_request(frame)?;
        ctx.get(request.session_id(), request.key()).await
    }
}
