use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{decode_range_scan_request, Frame, MessageType};

use super::MessageHandler;
use crate::async_func_task::HandleResult;
use crate::request_ctx::RequestCtx;

pub(crate) struct RangeScanHandler;

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
