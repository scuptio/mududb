use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{decode_put_request, Frame, MessageType};

use super::MessageHandler;
use crate::async_func_task::HandleResult;
use crate::request_ctx::RequestCtx;

pub(crate) struct PutHandler;

#[async_trait]
impl MessageHandler for PutHandler {
    fn message_type(&self) -> MessageType {
        MessageType::Put
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let request = decode_put_request(frame)?;
        let session_id = request.session_id();
        let (key, value) = request.into_parts();
        ctx.put(session_id, key, value).await
    }
}
