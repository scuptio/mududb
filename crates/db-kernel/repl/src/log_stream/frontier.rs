//! Durable applied-frontier store of one control stream's business machine.
//!
//! The frontier records how far the business state machine has applied its
//! stream, plus an opaque business checkpoint (the epoch stream carries its
//! last applied `(stable, current)` here so a restart never regresses the
//! shared epoch state). Publication is atomic: write temp file, fsync,
//! rename, fsync the directory.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{StreamId, StreamProgress};
use serde::{Deserialize, Serialize};

const FRONTIER_VERSION: u32 = 1;
const MAX_FRONTIER_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct FrontierStruct {
    version: u32,
    stream_id: u64,
    stream_generation: u64,
    end_lsn: u64,
    segment_digest: [u8; 32],
    business: Vec<u8>,
}

/// One stream's durable applied frontier plus its business checkpoint.
pub struct FrontierStore {
    path: std::path::PathBuf,
    stream_id: StreamId,
    stream_generation: u64,
}

impl FrontierStore {
    /// Opens the store; a missing file means nothing was applied yet.
    /// A mismatched stream identity fails closed.
    pub async fn open(
        dir: &std::path::Path,
        file_name: &str,
        stream_id: StreamId,
        stream_generation: u64,
    ) -> RS<(Self, Option<(StreamProgress, Vec<u8>)>)> {
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::ensure_dir(fs.as_ref(), dir).await?;
        let store = Self {
            path: dir.join(file_name),
            stream_id,
            stream_generation,
        };
        let Some((progress, business)) = store.load(fs.as_ref()).await? else {
            return Ok((store, None));
        };
        Ok((store, Some((progress, business))))
    }

    /// The last durably applied prefix, when any.
    pub async fn load(
        &self,
        fs: &dyn mudu_sys::contract::async_fs::AsyncFs,
    ) -> RS<Option<(StreamProgress, Vec<u8>)>> {
        if !fs.path_exists(&self.path).await? {
            return Ok(None);
        }
        if fs.metadata_len(&self.path).await? > MAX_FRONTIER_BYTES {
            return Err(invalid("applied frontier file exceeds its size budget"));
        }
        let record: FrontierStruct = decode(&fs.read_all(&self.path).await?)?;
        if record.version != FRONTIER_VERSION
            || record.stream_id != self.stream_id.0
            || record.stream_generation != self.stream_generation
        {
            return Err(invalid("applied frontier belongs to a different stream"));
        }
        Ok(Some((
            StreamProgress {
                stream_id: self.stream_id,
                stream_generation: self.stream_generation,
                end_lsn: record.end_lsn,
                segment_digest: record.segment_digest,
            },
            record.business,
        )))
    }

    /// Atomically persists the applied prefix and its business checkpoint.
    /// Callers must persist the business effect first; the frontier is the
    /// durable witness that the effect happened.
    pub async fn store(&self, progress: &StreamProgress, business: &[u8]) -> RS<()> {
        if progress.stream_id != self.stream_id
            || progress.stream_generation != self.stream_generation
        {
            return Err(invalid("applied frontier stream identity mismatch"));
        }
        let record = FrontierStruct {
            version: FRONTIER_VERSION,
            stream_id: self.stream_id.0,
            stream_generation: self.stream_generation,
            end_lsn: progress.end_lsn,
            segment_digest: progress.segment_digest,
            business: business.to_vec(),
        };
        let encoded = encode(&record)?;
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::publish(fs.as_ref(), &self.path, &encoded).await
    }
}

/// Reads the business checkpoint of a stored frontier record without opening
/// the store (startup truncation reads it before the group exists). The
/// record is validated by version; the file's path pins the stream identity.
pub async fn read_frontier_business(path: &std::path::Path) -> RS<Option<Vec<u8>>> {
    let fs = mudu_sys::fs::tokio_fs();
    if !fs.path_exists(path).await? {
        return Ok(None);
    }
    if fs.metadata_len(path).await? > MAX_FRONTIER_BYTES {
        return Err(invalid("applied frontier file exceeds its size budget"));
    }
    let record: FrontierStruct = decode(&fs.read_all(path).await?)?;
    if record.version != FRONTIER_VERSION {
        return Err(invalid("applied frontier version mismatch"));
    }
    Ok(Some(record.business))
}

pub(crate) fn encode<T: Serialize>(value: &T) -> RS<Vec<u8>> {
    rmp_serde::to_vec(value).map_err(|e| mudu_error!(ErrorCode::Encode, "encode frontier", e))
}

pub(crate) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> RS<T> {
    rmp_serde::from_slice(bytes).map_err(|e| mudu_error!(ErrorCode::Decode, "decode frontier", e))
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}
