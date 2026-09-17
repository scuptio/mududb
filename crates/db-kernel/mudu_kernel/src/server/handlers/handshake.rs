use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{
    decode_handshake_request, encode_handshake_response, Frame, HandshakeResponse, MessageType,
};

use crate::server::async_func_task::HandleResult;
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;

pub(in crate::server) struct HandshakeHandler;

#[async_trait]
impl MessageHandler for HandshakeHandler {
    fn message_type(&self) -> MessageType {
        MessageType::Handshake
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let request = decode_handshake_request(frame)?;
        let selected = request
            .supported_versions
            .into_iter()
            .max()
            .filter(|v| *v == 1u32)
            .ok_or_else(|| {
                mudu_error!(ErrorCode::Parse, "no mutually supported protocol version")
            })?;

        let response = HandshakeResponse {
            selected_version: selected,
            capabilities: vec![
                "protocol.handshake".to_string(),
                "result.table.v1".to_string(),
            ],
        };
        Ok(HandleResult::Response(encode_handshake_response(
            ctx.request_id(),
            &response,
        )?))
    }
}
