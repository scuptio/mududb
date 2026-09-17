use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{decode_range_scan_request, Frame, MessageType};

use crate::server::async_func_task::HandleResult;
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;

pub(in crate::server) struct RangeScanHandler;

#[async_trait]
impl MessageHandler for RangeScanHandler {
    fn message_type(&self) -> MessageType {
        MessageType::RangeScan
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let request = decode_range_scan_request(frame)?;
        ctx.range_scan(request.session_id(), request.start_key(), request.end_key())
            .await
    }
}
