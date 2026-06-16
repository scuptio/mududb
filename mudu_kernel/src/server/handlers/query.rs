use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{
    decode_client_request, Frame, MessageType, ServerPerfDigest,
};
use mudu_sys::perf::{PerfSpan, TxnStage};
use mudu_sys::time::instant_now;

use crate::server::async_func_task::HandleResult;
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;

pub(in crate::server) struct QueryHandler;

#[async_trait]
impl MessageHandler for QueryHandler {
    fn message_type(&self) -> MessageType {
        MessageType::Query
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let trace_context = frame.header().trace_context();
        let trace_id = trace_context.trace_id;

        let (request, recv_ns) = {
            let recv_start = instant_now();
            let _recv = PerfSpan::new(TxnStage::NetworkRecv, trace_id);
            let request = decode_client_request(frame)?;
            (request, recv_start.elapsed().as_nanos() as u64)
        };

        let mut digest = ServerPerfDigest::new(trace_id);
        digest.set(TxnStage::NetworkRecv, recv_ns);

        ctx.query(
            request.oid() as _,
            request.app_name(),
            request.sql(),
            Some(digest),
        )
        .await
    }
}
