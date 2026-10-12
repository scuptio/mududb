use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{decode_session_create_request, Frame, MessageType};

use super::MessageHandler;
use crate::async_func_task::HandleResult;
use crate::request_ctx::RequestCtx;
pub(crate) struct SessionCreateHandler;

#[async_trait]
impl MessageHandler for SessionCreateHandler {
    fn message_type(&self) -> MessageType {
        MessageType::SessionCreate
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let request = decode_session_create_request(frame)?;
        let config = ctx.parse_session_open_config(request.config_json())?;
        ctx.session_create(config).await
    }
}
