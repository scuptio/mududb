use crate::server::async_func_task::HandleResult;
use crate::server::message_dispatcher::MessageDispatcher;
use crate::server::request_ctx::RequestCtx;
use crate::server::session_bound_worker_runtime::new_session_bound_worker_runtime;
use crate::server::worker::WorkerRuntime;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{Frame, FrameHeader, MessageType, HEADER_LEN};
use mudu_sys::scoped_task_trace;

pub fn try_decode_next_frame(buf: &[u8]) -> RS<Option<(Frame, usize)>> {
    if buf.len() < HEADER_LEN {
        return Ok(None);
    }
    let header = FrameHeader::decode_header_bytes(&buf[..HEADER_LEN])?;
    let payload_len = header.payload_len() as usize;
    let frame_len = HEADER_LEN + payload_len;
    if buf.len() < frame_len {
        return Ok(None);
    }
    let frame = Frame::decode(&buf[..frame_len])?;
    Ok(Some((frame, frame_len)))
}

pub async fn dispatch_frame_async(
    worker: &WorkerRuntime,
    conn_id: u64,
    frame: &Frame,
) -> RS<HandleResult> {
    scoped_task_trace!();
    let ctx = RequestCtx::new(
        new_session_bound_worker_runtime(worker.clone(), 0),
        conn_id,
        frame.header().request_id(),
    );
    if let Some(result) = MessageDispatcher::global().dispatch(&ctx, frame).await {
        return result;
    }
    match frame.header().message_type() {
        MessageType::Get
        | MessageType::Put
        | MessageType::RangeScan
        | MessageType::Query
        | MessageType::Execute
        | MessageType::Batch
        | MessageType::ProcedureInvoke
        | MessageType::SessionCreate
        | MessageType::SessionClose => unreachable!(),
        MessageType::Handshake | MessageType::Auth | MessageType::Response | MessageType::Error => {
            Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unsupported client message type {:?}",
                    frame.header().message_type()
                )
            ))
        }
    }
}
