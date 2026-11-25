//! Durable external records using the existing LGFM staging and checksum chain.

use super::metrics::StreamMetrics;
use super::state::invalid;
use super::StreamStruct;
use crate::seg_hash::{seg_hash_chain, ZERO_SEG_HASH};
use crate::staging::StagingStore;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{GroupIdobject, StreamDescriptor, StreamId, StreamProgress};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::Mutex;
use wal::log_frame::{deserialize_entry, frame_lsn, serialize_entry, split_frame};

/// Stable external record format. Digest version 1 reuses the existing CRC64
/// chain for accidental corruption detection; it is not a cryptographic hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamEnvelope {
    /// Envelope format version (currently one).
    pub version: u32,
    /// Group authority owning this record.
    pub identity: GroupIdobject,
    /// Stream identity and incarnation.
    pub stream_id: StreamId,
    /// Stream incarnation.
    pub stream_generation: u64,
    /// Stable retry identity, durably stored outside Raft.
    pub request_id: u128,
    /// Explicit cross-stream applied prerequisites.
    pub dependencies: Vec<StreamProgress>,
    /// Adapter-encoded operation.
    pub payload: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct LogIdobject {
    version: u32,
    identity: GroupIdobject,
    descriptor: StreamDescriptor,
}

/// The durable GC anchor: the prefix below `base_lsn` has been reclaimed;
/// `base_digest` is the chain digest of the record at `base_lsn - 1` (the
/// chain anchor of the first retained record).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct GcAnchor {
    version: u32,
    base_lsn: u64,
    base_digest: [u8; 32],
}

struct LogIndex {
    /// First retained LSN (0 before any prefix GC).
    base_lsn: u64,
    /// Chain digest of the record at `base_lsn - 1` (ZERO at base 0).
    base_digest: [u8; 32],
    /// Progress of retained records; `progress[i]` has LSN `base_lsn + i`.
    progress: Vec<StreamProgress>,
    requests: BTreeMap<u128, u64>,
    failed: bool,
}

/// Single-owner durable log for one registered stream. A cancelled or failed
/// mutation fails closed until reopening. Prefix GC reclaims records below a
/// snapshot-protected frontier; a peer needing reclaimed bytes must install a
/// snapshot instead (group-level recovery evidence).
pub struct DurableStreamLog {
    identity: GroupIdobject,
    descriptor: StreamDescriptor,
    staging: StagingStore,
    index: Mutex<LogIndex>,
    /// In-flight read floor: the lowest retained LSN a reader still needs.
    /// `0` means no reader is pinned; `gc_prefix` clamps its frontier to it.
    read_floor: Arc<AtomicU64>,
}

/// Clears a stream log's read floor when the owning read/apply round ends.
#[must_use = "the read floor stays pinned until this guard drops"]
pub struct ReadFloorGuard {
    read_floor: Arc<AtomicU64>,
}

impl Drop for ReadFloorGuard {
    fn drop(&mut self) {
        self.read_floor
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

impl DurableStreamLog {
    /// Opens and validates every retained frame, then fsyncs the recovered prefix.
    /// The immutable manifest prevents accidentally reusing another stream's path.
    pub async fn open(
        dir: PathBuf,
        identity: GroupIdobject,
        descriptor: StreamDescriptor,
    ) -> RS<Self> {
        if identity.generation == 0
            || descriptor.stream_generation == 0
            || descriptor.codec_version == 0
            || descriptor.digest_version != 1
            || descriptor.budget.max_record_bytes == 0
            || descriptor.budget.max_record_bytes > 16 * 1024 * 1024
        {
            return Err(invalid(
                "unsupported stream storage identity, codec, digest or record budget",
            ));
        }
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::ensure_dir(fs.as_ref(), &dir).await?;
        let manifest = dir.join("stream.identity");
        if fs.path_exists(&manifest).await? {
            if fs.metadata_len(&manifest).await? > 16384 {
                return Err(invalid("oversized stream identity"));
            }
            let saved: LogIdobject = decode(&fs.read_all(&manifest).await?)?;
            if saved.version != 1 || saved.identity != identity || saved.descriptor != descriptor {
                return Err(invalid("stream storage identity mismatch"));
            }
        } else {
            if fs
                .read_dir(&dir)
                .await?
                .iter()
                .any(|p| p.extension().is_some_and(|s| s == "xls"))
            {
                return Err(invalid(
                    "unsupported stream storage: identity manifest is missing",
                ));
            }
            crate::durable_io::publish(
                fs.as_ref(),
                &manifest,
                &encode(&LogIdobject {
                    version: 1,
                    identity,
                    descriptor: descriptor.clone(),
                })?,
            )
            .await?;
        }
        let anchor = Self::load_anchor(&dir).await?;
        let staging = StagingStore::open(dir, fs).await?;
        let log = Self {
            identity,
            descriptor,
            staging,
            index: Mutex::new(LogIndex {
                base_lsn: anchor.map(|a| a.base_lsn).unwrap_or(0),
                base_digest: anchor.map(|a| a.base_digest).unwrap_or(ZERO_SEG_HASH),
                progress: Vec::new(),
                requests: BTreeMap::new(),
                failed: false,
            }),
            read_floor: Arc::new(AtomicU64::new(0)),
        };
        // Rebuild the in-memory index from the retained durable frames.
        let base = log.index.lock().await.base_lsn;
        if let Some(last) = log.staging.max_lsn()? {
            let mut index = log.index.lock().await;
            for lsn in base..=last.as_u64() {
                let frame = log
                    .staging
                    .frame_bytes(lsn)
                    .await?
                    .ok_or_else(|| invalid("missing stream frame"))?;
                log.index_frame(&mut index, &frame)?;
            }
        }
        log.staging.flush().await?;
        Ok(log)
    }

    /// Waits for underlying I/O after cancellation or before shutdown/reopen.
    pub async fn drain(&self) -> RS<()> {
        self.staging.drain().await
    }

    /// Pins the read floor so `gc_prefix` cannot reclaim below `lsn`. There is
    /// at most one live reader per stream, so a plain store is sufficient.
    pub fn pin_read(&self, lsn: u64) -> ReadFloorGuard {
        self.read_floor
            .store(lsn, std::sync::atomic::Ordering::Relaxed);
        ReadFloorGuard {
            read_floor: self.read_floor.clone(),
        }
    }

    /// Group namespace bound by the durable manifest.
    pub fn identity(&self) -> GroupIdobject {
        self.identity
    }

    /// Immutable adapter identity and format.
    pub fn descriptor(&self) -> &StreamDescriptor {
        &self.descriptor
    }

    /// Appends and fsyncs an operation; identical retries return the original
    /// prefix and bytes. This is durability only, never business visibility.
    pub async fn append(
        &self,
        request_id: u128,
        payload: Vec<u8>,
        dependencies: Vec<StreamProgress>,
    ) -> RS<(StreamProgress, Vec<u8>)> {
        let body = StreamEnvelope {
            version: 1,
            identity: self.identity,
            stream_id: self.descriptor.stream_id,
            stream_generation: self.descriptor.stream_generation,
            request_id,
            dependencies,
            payload,
        };
        self.validate(&body)?;
        let mut index = self.index.lock().await;
        healthy(&index)?;
        if let Some(&lsn) = index.requests.get(&request_id) {
            let frame = self.frame_at(&index, lsn).await?;
            if deserialize_entry::<StreamEnvelope>(std::slice::from_ref(&frame))? != body {
                return Err(invalid("stream request id reused with different contents"));
            }
            return Ok((
                index.progress[(lsn - index.base_lsn) as usize].clone(),
                frame,
            ));
        }
        let lsn = index.base_lsn + index.progress.len() as u64;
        let size = encode(&body)?
            .len()
            .checked_add(32)
            .ok_or_else(|| invalid("record size overflow"))?;
        let frames = serialize_entry(&body, size, &AtomicU64::new(lsn))?;
        if frames.len() != 1 {
            return Err(invalid("stream record must fit one frame"));
        }
        let frame = frames
            .into_iter()
            .next()
            .ok_or_else(|| invalid("empty encoded record"))?;
        self.persist(&mut index, &frame).await?;
        StreamMetrics::global().note_append(self.identity.group_id, self.descriptor.stream_id);
        Ok((
            index.progress[(lsn - index.base_lsn) as usize].clone(),
            frame,
        ))
    }

    /// Accepts one ordered, bounded peer frame. An existing LSN must be byte
    /// identical; divergence requires authoritative heal, never blind
    /// overwrite. Frames below the GC frontier are already covered.
    pub async fn receive(&self, frame: &[u8]) -> RS<StreamProgress> {
        self.decode_frame(frame)?;
        let lsn = frame_lsn(frame)?.as_u64();
        let mut index = self.index.lock().await;
        healthy(&index)?;
        if lsn < index.base_lsn {
            return self
                .frontier_locked(&index)
                .await?
                .ok_or_else(|| invalid("missing durable frame"));
        }
        if let Some(existing) = index.progress.get((lsn - index.base_lsn) as usize) {
            if self.frame_at(&index, lsn).await? != frame {
                return Err(invalid("divergent stream frame"));
            }
            return Ok(existing.clone());
        }
        self.persist(&mut index, frame).await?;
        index
            .progress
            .last()
            .cloned()
            .ok_or_else(|| invalid("missing durable frame"))
    }

    /// Finds an existing durable retry identity. The caller must separately
    /// require this prefix to be committed before returning a successful result.
    pub async fn request_progress(&self, request_id: u128) -> RS<Option<StreamProgress>> {
        let index = self.index.lock().await;
        healthy(&index)?;
        Ok(index
            .requests
            .get(&request_id)
            .map(|lsn| index.progress[(*lsn - index.base_lsn) as usize].clone()))
    }

    /// Truncates only after the caller establishes an authoritative leader
    /// barrier. A provided retained prefix must match the durable digest.
    pub async fn truncate_uncommitted(&self, committed: Option<&StreamProgress>) -> RS<()> {
        let mut index = self.index.lock().await;
        healthy(&index)?;
        let cut = if let Some(prefix) = committed {
            if prefix.end_lsn < index.base_lsn {
                // The prefix is reclaimed history: either it anchors the
                // chain exactly, or it cannot be verified here at all.
                if prefix.end_lsn.saturating_add(1) == index.base_lsn
                    && prefix.segment_digest == index.base_digest
                {
                    index.base_lsn
                } else if prefix.end_lsn.saturating_add(1) < index.base_lsn {
                    return Err(invalid(
                        "cannot retain a committed prefix below the GC frontier",
                    ));
                } else {
                    index.base_lsn
                }
            } else {
                if index
                    .progress
                    .get((prefix.end_lsn - index.base_lsn) as usize)
                    != Some(prefix)
                {
                    return Err(invalid(
                        "cannot retain a missing or divergent committed prefix",
                    ));
                }
                prefix
                    .end_lsn
                    .checked_add(1)
                    .ok_or_else(|| invalid("stream LSN exhausted"))?
            }
        } else {
            index.base_lsn
        };
        let len = index.base_lsn + index.progress.len() as u64;
        if cut == len {
            return Ok(());
        }
        index.failed = true;
        self.staging.truncate_from(cut).await?;
        let keep = (cut - index.base_lsn) as usize;
        index.progress.truncate(keep);
        index.requests.retain(|_, lsn| *lsn < cut);
        index.failed = false;
        Ok(())
    }

    /// Reclaims every record below `below_lsn` (the prefix mirror of
    /// `truncate_uncommitted`): publishes the new GC anchor durably first
    /// (so a crash mid-GC reopens consistently), drops the staging frames
    /// and shrinks the in-memory index. The caller must guarantee the prefix
    /// is covered by a business snapshot; a peer needing reclaimed bytes
    /// must install one instead of fetching.
    pub async fn gc_prefix(&self, below_lsn: u64) -> RS<u64> {
        // A live reader pins the floor: never reclaim below its in-flight read
        // position (the commit/apply driver may still need those records).
        let floor = self.read_floor.load(std::sync::atomic::Ordering::Relaxed);
        let below_lsn = if floor > 0 {
            below_lsn.min(floor)
        } else {
            below_lsn
        };
        let mut index = self.index.lock().await;
        healthy(&index)?;
        let end = index.base_lsn + index.progress.len() as u64;
        if below_lsn <= index.base_lsn {
            return Ok(0);
        }
        if below_lsn > end {
            return Err(invalid("cannot GC beyond the durable frontier"));
        }
        let new_base_digest = if below_lsn == 0 {
            ZERO_SEG_HASH
        } else {
            index.progress[(below_lsn - 1 - index.base_lsn) as usize].segment_digest
        };
        // Publish the anchor before any deletion: a crash after deletion but
        // before the anchor would leave a prefix-less log without its chain
        // anchor.
        let anchor = GcAnchor {
            version: 1,
            base_lsn: below_lsn,
            base_digest: new_base_digest,
        };
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::publish(fs.as_ref(), &self.anchor_path(), &encode(&anchor)?).await?;
        let reclaimed = self.staging.gc_prefix(below_lsn).await?;
        index.failed = true;
        let drop_count = (below_lsn - index.base_lsn) as usize;
        index.progress.drain(..drop_count);
        index.requests.retain(|_, lsn| *lsn >= below_lsn);
        index.base_lsn = below_lsn;
        index.base_digest = new_base_digest;
        index.failed = false;
        StreamMetrics::global().note_gc(
            self.identity.group_id,
            self.descriptor.stream_id,
            reclaimed,
        );
        Ok(reclaimed)
    }

    /// Reads a verified durable record and its stable request identity.
    pub async fn record(&self, lsn: u64) -> RS<(u128, StreamStruct)> {
        let index = self.index.lock().await;
        healthy(&index)?;
        if lsn < index.base_lsn {
            return Err(invalid(
                "stream record is below the GC frontier; install a snapshot",
            ));
        }
        let progress = index
            .progress
            .get((lsn - index.base_lsn) as usize)
            .ok_or_else(|| invalid("missing durable record"))?
            .clone();
        let frame = self.frame_at(&index, lsn).await?;
        let previous = if lsn == index.base_lsn {
            index.base_digest
        } else {
            index.progress[(lsn - index.base_lsn - 1) as usize].segment_digest
        };
        if seg_hash_chain(&previous, std::slice::from_ref(&frame)) != progress.segment_digest {
            return Err(invalid("durable stream record changed after recovery"));
        }
        let body = self.decode_frame(&frame)?;
        Ok((
            body.request_id,
            StreamStruct {
                identity: body.identity,
                progress,
                dependencies: body.dependencies,
                payload: body.payload,
            },
        ))
    }

    /// Reads the durable LGFM frame for bounded transport or healing.
    pub async fn read_frame(&self, lsn: u64) -> RS<Vec<u8>> {
        let index = self.index.lock().await;
        healthy(&index)?;
        if lsn < index.base_lsn {
            return Err(invalid("stream frame is below the GC frontier"));
        }
        if index
            .progress
            .get((lsn - index.base_lsn) as usize)
            .is_none()
        {
            return Err(invalid("missing durable frame"));
        }
        self.frame_at(&index, lsn).await
    }

    /// The durable progress at exactly `lsn`, when retained (no frame read).
    pub async fn progress_at(&self, lsn: u64) -> RS<Option<StreamProgress>> {
        let index = self.index.lock().await;
        healthy(&index)?;
        if lsn < index.base_lsn {
            return Ok(None);
        }
        Ok(index.progress.get((lsn - index.base_lsn) as usize).cloned())
    }

    /// Checks an exact prefix against the chain rebuilt from durable frames.
    /// A prefix ending exactly at the GC frontier matches by its anchor digest.
    pub async fn contains(&self, prefix: &StreamProgress) -> RS<bool> {
        let index = self.index.lock().await;
        healthy(&index)?;
        if prefix.end_lsn < index.base_lsn {
            return Ok(prefix.end_lsn.saturating_add(1) == index.base_lsn
                && prefix.segment_digest == index.base_digest);
        }
        Ok(index
            .progress
            .get((prefix.end_lsn - index.base_lsn) as usize)
            == Some(prefix))
    }

    /// Highest locally durable prefix, including any speculative suffix.
    pub async fn frontier(&self) -> RS<Option<StreamProgress>> {
        let index = self.index.lock().await;
        healthy(&index)?;
        self.frontier_locked(&index).await
    }

    async fn frontier_locked(&self, index: &LogIndex) -> RS<Option<StreamProgress>> {
        Ok(index.progress.last().cloned())
    }

    async fn frame_at(&self, index: &LogIndex, lsn: u64) -> RS<Vec<u8>> {
        if lsn < index.base_lsn {
            return Err(invalid("stream frame is below the GC frontier"));
        }
        self.staging
            .frame_bytes(lsn)
            .await?
            .ok_or_else(|| invalid("missing stream frame"))
    }

    fn anchor_path(&self) -> PathBuf {
        // The staging store owns the directory; the anchor lives beside the
        // identity manifest.
        self.staging.dir().join("stream.gc")
    }

    async fn load_anchor(dir: &Path) -> RS<Option<GcAnchor>> {
        let fs = mudu_sys::fs::tokio_fs();
        let path = dir.join("stream.gc");
        if !fs.path_exists(&path).await? {
            return Ok(None);
        }
        if fs.metadata_len(&path).await? > 4096 {
            return Err(invalid("oversized stream GC anchor"));
        }
        let anchor: GcAnchor = decode(&fs.read_all(&path).await?)?;
        if anchor.version != 1 {
            return Err(invalid("unsupported stream GC anchor version"));
        }
        Ok(Some(anchor))
    }

    async fn persist(&self, index: &mut LogIndex, frame: &[u8]) -> RS<()> {
        let body = self.decode_frame(frame)?;
        if frame_lsn(frame)?.as_u64() != index.base_lsn + index.progress.len() as u64
            || index.requests.contains_key(&body.request_id)
        {
            return Err(invalid("noncontiguous or duplicate stream record"));
        }
        index.failed = true;
        self.staging.append_frames(&[frame.to_vec()]).await?;
        self.staging.flush().await?;
        self.index_frame(index, frame)?;
        index.failed = false;
        Ok(())
    }

    fn index_frame(&self, index: &mut LogIndex, frame: &[u8]) -> RS<()> {
        let body = self.decode_frame(frame)?;
        let lsn = frame_lsn(frame)?.as_u64();
        if lsn != index.base_lsn + index.progress.len() as u64
            || index.requests.contains_key(&body.request_id)
        {
            return Err(invalid(
                "noncontiguous or duplicate recovered stream record",
            ));
        }
        let previous = index
            .progress
            .last()
            .map_or(index.base_digest, |p| p.segment_digest);
        index.progress.push(StreamProgress {
            stream_id: self.descriptor.stream_id,
            stream_generation: self.descriptor.stream_generation,
            end_lsn: lsn,
            segment_digest: seg_hash_chain(&previous, &[frame.to_vec()]),
        });
        index.requests.insert(body.request_id, lsn);
        Ok(())
    }

    fn decode_frame(&self, frame: &[u8]) -> RS<StreamEnvelope> {
        decode_snapshot_frame(self.identity, &self.descriptor, frame)
    }

    fn validate(&self, body: &StreamEnvelope) -> RS<()> {
        validate_envelope(self.identity, &self.descriptor, body)
    }
}

fn decode_snapshot_frame(
    identity: GroupIdobject,
    descriptor: &StreamDescriptor,
    frame: &[u8],
) -> RS<StreamEnvelope> {
    if frame.len() > descriptor.budget.max_record_bytes as usize + 16384 {
        return Err(invalid("stream frame exceeds record budget"));
    }
    let (header, _, _) = split_frame(frame)?;
    if header.n_part() != 0 {
        return Err(invalid("fragmented stream envelope"));
    }
    let body = deserialize_entry(&[frame.to_vec()])?;
    validate_envelope(identity, descriptor, &body)?;
    Ok(body)
}

fn validate_envelope(
    identity: GroupIdobject,
    descriptor: &StreamDescriptor,
    body: &StreamEnvelope,
) -> RS<()> {
    if body.version != 1
        || body.identity != identity
        || body.stream_id != descriptor.stream_id
        || body.stream_generation != descriptor.stream_generation
        || body.payload.len() > descriptor.budget.max_record_bytes as usize
        || body.dependencies.len() > descriptor.requires.len()
    {
        return Err(invalid("invalid stream envelope"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for dependency in &body.dependencies {
        if !descriptor.requires.contains(&dependency.stream_id)
            || dependency.stream_generation == 0
            || !seen.insert(dependency.stream_id)
        {
            return Err(invalid("invalid stream record dependency"));
        }
    }
    Ok(())
}

fn healthy(index: &LogIndex) -> RS<()> {
    if index.failed {
        Err(invalid(
            "stream log mutation failed or cancelled; reopen required",
        ))
    } else {
        Ok(())
    }
}
fn encode<T: Serialize>(value: &T) -> RS<Vec<u8>> {
    rmp_serde::to_vec(value).map_err(|e| mudu_error!(ErrorCode::Encode, "encode stream log", e))
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> RS<T> {
    rmp_serde::from_slice(bytes).map_err(|e| mudu_error!(ErrorCode::Decode, "decode stream log", e))
}

mod snapshot;
pub use snapshot::StreamLogSnapshot;
