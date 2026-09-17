use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{decode_client_request, Frame, MessageType};

use crate::server::async_func_task::HandleResult;
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;

pub(in crate::server) struct BatchHandler;

#[async_trait]
impl MessageHandler for BatchHandler {
    fn message_type(&self) -> MessageType {
        MessageType::Batch
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let request = decode_client_request(frame)?;
        ctx.batch(request.oid() as _, request.app_name(), request.sql())
            .await
    }
}
