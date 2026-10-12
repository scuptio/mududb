use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{
    decode_raft_append_request, decode_raft_append_response, decode_raft_apply_request,
    decode_raft_apply_response, decode_raft_pre_vote_request, decode_raft_pre_vote_response,
    decode_raft_update_conf_request, decode_raft_update_conf_response, decode_raft_vote_request,
    decode_raft_vote_response, encode_raft_append_request, encode_raft_append_response,
    encode_raft_apply_request, encode_raft_apply_response, encode_raft_pre_vote_request,
    encode_raft_pre_vote_response, encode_raft_update_conf_request,
    encode_raft_update_conf_response, encode_raft_vote_request, encode_raft_vote_response, Frame,
    MessageType, RaftAppendReqFrame, RaftAppendRespFrame, RaftApplyReqFrame, RaftApplyRespFrame,
    RaftLogEntryWire, RaftPreVoteReqFrame, RaftPreVoteRespFrame, RaftUpdateConfReqFrame,
    RaftUpdateConfRespFrame, RaftVoteReqFrame, RaftVoteRespFrame,
};
use mudu_raft::core::conf::{ConfNodeValue, ConfVersion};
use mudu_raft::core::message::{LogEntry, RaftMsg};
use mudu_raft::ReplEntry;

/// Encodes one raft message as a full MSFM `Raft*` frame (request id 0; the
/// raft channel carries no request correlation).
pub(crate) fn raft_msg_to_frame_bytes(node_id: u64, msg: &RaftMsg) -> RS<Vec<u8>> {
    match msg {
        RaftMsg::PreVoteReq(m) => encode_raft_pre_vote_request(
            0,
            &RaftPreVoteReqFrame::new(
                m.source_nid,
                m.request_term,
                m.last_log_term,
                m.last_log_index,
            ),
        ),
        RaftMsg::PreVoteResp(m) => encode_raft_pre_vote_response(
            0,
            &RaftPreVoteRespFrame::new(m.source_nid, m.request_term, m.vote_granted),
        ),
        RaftMsg::VoteReq(m) => encode_raft_vote_request(
            0,
            &RaftVoteReqFrame::new(node_id, m.term, m.last_log_term, m.last_log_index),
        ),
        RaftMsg::VoteResp(m) => {
            encode_raft_vote_response(0, &RaftVoteRespFrame::new(node_id, m.term, m.vote_granted))
        }
        RaftMsg::AppendReq(m) => encode_raft_append_request(
            0,
            &RaftAppendReqFrame::new(
                node_id,
                m.term,
                m.prev_log_index,
                m.prev_log_term,
                encode_log_entries(&m.entries)?,
                m.commit_index,
            ),
        ),
        RaftMsg::AppendResp(m) => encode_raft_append_response(
            0,
            &RaftAppendRespFrame::new(
                node_id,
                m.term,
                m.append_success,
                m.commit_index,
                m.match_index,
                m.next_index,
            ),
        ),
        RaftMsg::ApplyReq(m) => encode_raft_apply_request(
            0,
            &RaftApplyReqFrame::new(
                node_id,
                m.term,
                m.id.clone(),
                m.begin_index,
                m.end_index,
                m.snapshot.index,
                m.snapshot.term,
            ),
        ),
        RaftMsg::ApplyResp(m) => encode_raft_apply_response(
            0,
            &RaftApplyRespFrame::new(node_id, m.term, m.match_index, m.id.clone()),
        ),
        RaftMsg::UpdateConfReq(m) => encode_raft_update_conf_request(
            0,
            &RaftUpdateConfReqFrame::new(
                node_id,
                m.term,
                encode_conf_value(&m.conf_committed)?,
                encode_conf_value(&m.conf_new)?,
            ),
        ),
        RaftMsg::UpdateConfResp(m) => encode_raft_update_conf_response(
            0,
            &RaftUpdateConfRespFrame::new(
                node_id,
                m.term,
                encode_conf_version(&m.conf_committed)?,
                encode_conf_version(&m.conf_new)?,
            ),
        ),
    }
}

/// Decodes one inbound `Raft*` frame into `(src_node_id, RaftMsg)` for
/// `RaftHandle::deliver`. Returns `Ok(None)` for frame types that are not
/// raft traffic.
pub(crate) fn raft_frame_to_msg(frame: &Frame) -> RS<Option<(u64, RaftMsg)>> {
    let msg = match frame.header().message_type() {
        MessageType::RaftPreVoteReq => decode_pre_vote_req(frame)?,
        MessageType::RaftPreVoteResp => decode_pre_vote_resp(frame)?,
        MessageType::RaftVoteReq => decode_vote_req(frame)?,
        MessageType::RaftVoteResp => decode_vote_resp(frame)?,
        MessageType::RaftAppendReq => decode_append_req(frame)?,
        MessageType::RaftAppendResp => decode_append_resp(frame)?,
        MessageType::RaftApplyReq => decode_apply_req(frame)?,
        MessageType::RaftApplyResp => decode_apply_resp(frame)?,
        MessageType::RaftUpdateConfReq => decode_update_conf_req(frame)?,
        MessageType::RaftUpdateConfResp => decode_update_conf_resp(frame)?,
        _ => return Ok(None),
    };
    Ok(Some(msg))
}

fn decode_pre_vote_req(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_pre_vote_request(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::PreVoteReq(mudu_raft::core::message::PreVoteReq {
            source_nid: m.src_node_id(),
            request_term: m.request_term(),
            last_log_term: m.last_log_term(),
            last_log_index: m.last_log_index(),
        }),
    ))
}

fn decode_pre_vote_resp(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_pre_vote_response(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::PreVoteResp(mudu_raft::core::message::PreVoteResp {
            source_nid: m.src_node_id(),
            request_term: m.request_term(),
            vote_granted: m.vote_granted(),
        }),
    ))
}

fn decode_vote_req(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_vote_request(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::VoteReq(mudu_raft::core::message::VoteReq {
            term: m.term(),
            last_log_term: m.last_log_term(),
            last_log_index: m.last_log_index(),
        }),
    ))
}

fn decode_vote_resp(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_vote_response(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::VoteResp(mudu_raft::core::message::VoteResp {
            term: m.term(),
            vote_granted: m.vote_granted(),
        }),
    ))
}

fn decode_append_req(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_append_request(frame)?;
    let commit_index = m.commit_index();
    Ok((
        m.src_node_id(),
        RaftMsg::AppendReq(mudu_raft::core::message::AppendReq {
            term: m.term(),
            prev_log_index: m.prev_log_index(),
            prev_log_term: m.prev_log_term(),
            entries: decode_log_entries(m.into_entries())?,
            commit_index,
        }),
    ))
}

fn decode_append_resp(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_append_response(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::AppendResp(mudu_raft::core::message::AppendResp {
            term: m.term(),
            append_success: m.append_success(),
            commit_index: m.commit_index(),
            match_index: m.match_index(),
            next_index: m.next_index(),
        }),
    ))
}

fn decode_apply_req(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_apply_request(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::ApplyReq(mudu_raft::core::message::ApplyReq {
            term: m.term(),
            id: m.id().to_string(),
            begin_index: m.begin_index(),
            end_index: m.end_index(),
            snapshot: mudu_raft::core::message::Snapshot {
                index: m.snapshot_index(),
                term: m.snapshot_term(),
            },
        }),
    ))
}

fn decode_apply_resp(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_apply_response(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::ApplyResp(mudu_raft::core::message::ApplyResp {
            term: m.term(),
            match_index: m.match_index(),
            id: m.id().to_string(),
        }),
    ))
}

fn decode_update_conf_req(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_update_conf_request(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::UpdateConfReq(mudu_raft::core::message::UpdateConfReq {
            term: m.term(),
            conf_committed: decode_conf_value(m.conf_committed())?,
            conf_new: decode_conf_value(m.conf_new())?,
        }),
    ))
}

fn decode_update_conf_resp(frame: &Frame) -> RS<(u64, RaftMsg)> {
    let m = decode_raft_update_conf_response(frame)?;
    Ok((
        m.src_node_id(),
        RaftMsg::UpdateConfResp(mudu_raft::core::message::UpdateConfResp {
            term: m.term(),
            conf_committed: decode_conf_version(m.conf_committed())?,
            conf_new: decode_conf_version(m.conf_new())?,
        }),
    ))
}

fn encode_log_entries(entries: &[LogEntry]) -> RS<Vec<RaftLogEntryWire>> {
    entries
        .iter()
        .map(|entry| {
            let payload = rmp_serde::to_vec(&entry.value)
                .map_err(|e| mudu_error!(ErrorCode::Encode, "encode raft log entry error", e))?;
            Ok(RaftLogEntryWire::new(entry.term, entry.index, payload))
        })
        .collect()
}

fn decode_log_entries(entries: Vec<RaftLogEntryWire>) -> RS<Vec<LogEntry>> {
    entries
        .into_iter()
        .map(|wire| {
            let value: ReplEntry = rmp_serde::from_slice(wire.payload())
                .map_err(|e| mudu_error!(ErrorCode::Decode, "decode raft log entry error", e))?;
            Ok(LogEntry {
                term: wire.term(),
                index: wire.index(),
                value,
            })
        })
        .collect()
}

fn encode_conf_value(value: &ConfNodeValue) -> RS<Vec<u8>> {
    rmp_serde::to_vec(value)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode raft conf error", e))
}

fn decode_conf_value(bytes: &[u8]) -> RS<ConfNodeValue> {
    rmp_serde::from_slice(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode raft conf error", e))
}

fn encode_conf_version(version: &ConfVersion) -> RS<Vec<u8>> {
    rmp_serde::to_vec(version)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode raft conf version error", e))
}

fn decode_conf_version(bytes: &[u8]) -> RS<ConfVersion> {
    rmp_serde::from_slice(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode raft conf version error", e))
}
