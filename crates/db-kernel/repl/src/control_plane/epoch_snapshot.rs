//! Committed epoch state, coupled to complete external retry history.
use super::{frontier_checkpoint::EpochCheckpoint, streams::epoch_stream_descriptor};
use crate::log_stream::{
    frontier::{decode, encode},
    DurableStreamLog, FrontierStore, LoggedStreamSnapshot, StreamLogSnapshot,
};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupIdobject, StreamProgress};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct EpochImage {
    pub(super) version: u32,
    pub(super) prefix: Option<StreamProgress>,
    pub(super) checkpoint: EpochCheckpoint,
}
impl EpochImage {
    pub(super) fn empty() -> Self {
        Self {
            version: 1,
            prefix: None,
            checkpoint: EpochCheckpoint::initial(),
        }
    }

    pub(super) fn validate(&self, log: &StreamLogSnapshot) -> RS<()> {
        let mut checkpoint = EpochCheckpoint::initial();
        log.visit_records(|body, _| {
            checkpoint = checkpoint.clone().advance(decode(&body.payload)?)?;
            Ok(())
        })?;
        if self.version != 1
            || self.prefix.as_ref() != log.prefix()
            || !self.checkpoint.covers(&checkpoint)
        {
            return Err(invalid("epoch image regresses committed history"));
        }
        Ok(())
    }
}

/// Validated epoch snapshot. Local in-memory allocator watermarks are excluded;
/// only the committed state and complete retry history authorize restoration.
/// This does not install Raft, grant GC permission or admit client requests.
pub struct EpochSnapshot {
    identity: GroupIdobject,
    image: LoggedStreamSnapshot,
    business: EpochImage,
}
impl EpochSnapshot {
    /// Validates the manifest cut and replays epoch history before any writes.
    pub fn decode(
        bytes: &[u8],
        identity: GroupIdobject,
        prefix: Option<&StreamProgress>,
    ) -> RS<Self> {
        let image =
            LoggedStreamSnapshot::decode(bytes, identity, &epoch_stream_descriptor(), prefix)?;
        let business: EpochImage = decode(image.business())?;
        business.validate(image.log())?;
        Ok(Self {
            identity,
            image,
            business,
        })
    }

    /// Restores into exclusively owned offline state, before opening the control
    /// service. All group streams must be validated before calling this method.
    /// Identical retries resume after a partial log install; conflicting targets
    /// are rejected. Drain failed/cancelled log writes before reopening/retrying.
    pub async fn restore(&self, log: &DurableStreamLog, apply_dir: &Path) -> RS<()> {
        let descriptor = epoch_stream_descriptor();
        if log.identity() != self.identity || log.descriptor() != &descriptor {
            return Err(invalid("epoch restore target identity mismatch"));
        }
        let (frontier, existing) = FrontierStore::open(
            apply_dir,
            "epoch.frontier",
            descriptor.stream_id,
            descriptor.stream_generation,
        )
        .await?;
        let bytes = encode(&self.business.checkpoint)?;
        if let Some((prefix, checkpoint)) = existing {
            if Some(&prefix) != self.business.prefix.as_ref() || checkpoint != bytes {
                return Err(invalid(
                    "epoch restore would overwrite an existing frontier",
                ));
            }
        }
        log.restore_snapshot(self.image.log()).await?;
        if let Some(prefix) = &self.business.prefix {
            frontier.store(prefix, &bytes).await?;
        }
        Ok(())
    }
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}
