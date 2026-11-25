//! Local decision checkpoints. Publication never permits stream prefix GC:
//! request-id dedup and peer catch-up still depend on retained log records.
use super::{machine::TX_DECISION_STREAM, view::View};
use crate::log_stream::{
    frontier::{decode, encode},
    DurableStreamLog,
};
use catalog::contract::tx_decision::TxDecision;
use mudu::common::{crc::crc64, result::RS};
use mudu::{error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupIdobject, StreamProgress};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    identity: GroupIdobject,
    view: View,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    // CRC detects accidental corruption, not adversarial modification.
    crc: u64,
    payload: Vec<u8>,
}

pub(super) async fn store(path: &Path, identity: GroupIdobject, view: &View) -> RS<()> {
    if view.applied.is_none() {
        return Ok(());
    }
    let bytes = encode_image(identity, view)?;
    crate::durable_io::publish(mudu_sys::fs::tokio_fs().as_ref(), path, &bytes).await
}

pub(super) fn encode_image(identity: GroupIdobject, view: &View) -> RS<Vec<u8>> {
    validate(identity, view)?;
    let payload = encode(&Snapshot {
        version: 1,
        identity,
        view: view.clone(),
    })?;
    let bytes = encode(&Envelope {
        crc: crc64(&payload),
        payload,
    })?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("decision snapshot exceeds its size budget"));
    }
    Ok(bytes)
}

pub(super) async fn load(
    path: &Path,
    identity: GroupIdobject,
    log: &DurableStreamLog,
    frontier: Option<&(StreamProgress, Vec<u8>)>,
) -> RS<View> {
    let fs = mudu_sys::fs::tokio_fs();
    if !fs.path_exists(path).await? {
        return Ok(View::default());
    }
    if fs.metadata_len(path).await? > MAX_BYTES {
        return Err(invalid("decision snapshot exceeds its size budget"));
    }
    let view = decode_image(identity, &fs.read_all(path).await?)?;
    let prefix = view
        .applied
        .as_ref()
        .ok_or_else(|| invalid("decision snapshot has no applied prefix"))?;
    let (applied, _) =
        frontier.ok_or_else(|| invalid("decision snapshot has no durable apply witness"))?;
    if prefix.end_lsn > applied.end_lsn || !log.contains(prefix).await? {
        return Err(invalid(
            "decision snapshot is outside the applied durable history",
        ));
    }
    Ok(view)
}

pub(super) fn decode_image(identity: GroupIdobject, bytes: &[u8]) -> RS<View> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("decision snapshot exceeds its size budget"));
    }
    let envelope: Envelope = decode(bytes)?;
    if crc64(&envelope.payload) != envelope.crc {
        return Err(invalid("decision snapshot checksum mismatch"));
    }
    let snapshot: Snapshot = decode(&envelope.payload)?;
    if snapshot.version != 1 || snapshot.identity != identity {
        return Err(invalid("decision snapshot version or group mismatch"));
    }
    validate(identity, &snapshot.view)?;
    Ok(snapshot.view)
}

fn validate(identity: GroupIdobject, view: &View) -> RS<()> {
    if view.applied.is_none() && view.transactions.is_empty() && view.results.is_empty() {
        return Ok(());
    }

    let prefix = view
        .applied
        .as_ref()
        .ok_or_else(|| invalid("decision snapshot has no applied prefix"))?;
    if prefix.stream_id != TX_DECISION_STREAM || prefix.stream_generation != 1 {
        return Err(invalid("decision snapshot belongs to a different stream"));
    }
    if prefix.end_lsn.checked_add(1) != Some(view.results.len() as u64)
        || view.results.keys().copied().ne(0..=prefix.end_lsn)
    {
        return Err(invalid("decision snapshot has incomplete command results"));
    }
    for (id, state) in &view.transactions {
        let owner = &state.coordinator;
        if *id != owner.tx_id
            || owner.cluster_id != identity.cluster_id
            || owner.group_id != identity.group_id.0
            || owner.group_generation != identity.generation
            || state.participants.is_empty()
            || !state.finished.is_subset(&state.participants)
            || (state.decision.is_none() && !state.finished.is_empty())
            || (state.complete
                && (state.decision.is_none() || state.finished != state.participants))
            || matches!(state.decision, Some(TxDecision::Committed { commit_epoch }) if commit_epoch < state.snapshot_epoch)
        {
            return Err(invalid("decision snapshot contains an invalid transaction"));
        }
    }
    Ok(())
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}
