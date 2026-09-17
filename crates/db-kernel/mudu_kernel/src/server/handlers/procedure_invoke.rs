use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{
    decode_procedure_invoke_request, Frame, MessageType, ServerPerfDigest,
};
use mudu_sys::perf::{PerfSpan, TxnStage};
use mudu_sys::time::instant_now;

use crate::server::async_func_task::HandleResult;
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;

pub(in crate::server) struct ProcedureInvokeHandler;

#[async_trait]
impl MessageHandler for ProcedureInvokeHandler {
    fn message_type(&self) -> MessageType {
        MessageType::ProcedureInvoke
    }

    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult> {
        let trace_context = frame.header().trace_context();
        let trace_id = trace_context.trace_id;

        let (request, recv_ns) = {
            let recv_start = instant_now();
            let _recv = PerfSpan::new(TxnStage::NetworkRecv, trace_id);
            let request = decode_procedure_invoke_request(frame)?;
            (request, recv_start.elapsed().as_nanos() as u64)
        };

        let mut digest = ServerPerfDigest::new(trace_id);
        digest.set(TxnStage::NetworkRecv, recv_ns);

        ctx.invoke_procedure(request, Some(digest)).await
    }
}
