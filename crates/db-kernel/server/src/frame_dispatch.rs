use crate::async_func_task::HandleResult;
use crate::message_dispatcher::MessageDispatcher;
use crate::request_ctx::RequestCtx;
use crate::worker::new_session_bound_worker_runtime;
use crate::worker::WorkerRuntime;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{
    encode_merror_response, encode_merror_response_with_leader_hint, Frame, MessageType,
};
use mudu_sys::scoped_task_trace;

pub(crate) use ::catalog::frame::try_decode_next_frame;

/// Raft write gate: on a replication cluster, client write paths are only
/// served by the raft leader after the leader-elect heal completed
/// (`is_serving_leader`). Every other node (followers, candidates, a leader
/// still healing) rejects writes with an error response that carries the
/// hint of the current raft leader, so clients can redirect. Returns the
/// encoded error frame when the request is rejected, `None` when it may
/// proceed. On a single-node server (no raft share) everything proceeds.
///
/// Stream mode (`[replication.stream]`, no raft): only the configured
/// primary serves writes; replicas reject with a hint pointing at the
/// configured primary.
fn repl_write_gate(worker: &WorkerRuntime, frame: &Frame) -> RS<Option<Vec<u8>>> {
    match frame.header().message_type() {
        MessageType::Put
        | MessageType::Query
        | MessageType::Execute
        | MessageType::Batch
        | MessageType::ProcedureInvoke => {}
        _ => return Ok(None),
    }
    let request_id = frame.header().request_id();
    if let Some(share) = worker.raft_share() {
        if share.is_serving_leader() {
            return Ok(None);
        }
        let err = mudu_error!(
            ErrorCode::DomainViolation,
            "not the raft leader: write requests must be sent to the serving raft leader"
        );
        let payload = match share.leader_hint() {
            Some(hint) => encode_merror_response_with_leader_hint(request_id, &err, hint)?,
            None => encode_merror_response(request_id, &err)?,
        };
        return Ok(Some(payload));
    }
    if let Some(ctl) = worker.repl_ctl()? {
        if ctl.is_primary() {
            return Ok(None);
        }
        let err = mudu_error!(
            ErrorCode::DomainViolation,
            "not the primary: write requests must be sent to the configured primary"
        );
        let payload = match ctl.leader_hint() {
            Some(hint) => encode_merror_response_with_leader_hint(request_id, &err, hint.clone())?,
            None => encode_merror_response(request_id, &err)?,
        };
        return Ok(Some(payload));
    }
    Ok(None)
}

pub(crate) async fn dispatch_frame_async(
    worker: &WorkerRuntime,
    conn_id: u64,
    frame: &Frame,
) -> RS<HandleResult> {
    scoped_task_trace!();
    let _stage = crate::stage_stats::StageGuard::new(crate::stage_stats::Stage::FrameHandle);
    if let Some(rejection) = repl_write_gate(worker, frame)? {
        return Ok(HandleResult::Response(rejection));
    }
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
        MessageType::Handshake
        | MessageType::Auth
        | MessageType::Response
        | MessageType::Error
        | MessageType::ReplSubscribe
        | MessageType::ReplBatch
        | MessageType::ReplAck
        | MessageType::ReplHealFetch
        | MessageType::ReplHealResp
        | MessageType::ReplSnapshotOffer
        | MessageType::ReplSnapshotFetch
        | MessageType::ReplSnapshotResp
        | MessageType::ReplSubscribeAccept
        | MessageType::RaftPreVoteReq
        | MessageType::RaftPreVoteResp
        | MessageType::RaftVoteReq
        | MessageType::RaftVoteResp
        | MessageType::RaftAppendReq
        | MessageType::RaftAppendResp
        | MessageType::RaftApplyReq
        | MessageType::RaftApplyResp
        | MessageType::RaftUpdateConfReq
        | MessageType::RaftUpdateConfResp => Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "unsupported client message type {:?}",
                frame.header().message_type()
            )
        )),
    }
}
