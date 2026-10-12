//! Follower-side staging store for streamed WAL frames.
//!
//! Received leader frames land here first — plain chunked LGFM files under
//! `<log_dir>/repl_staging/worker_<i>/` — and only frames covered by a
//! raft-committed `CommitVec` are promoted into the worker WAL (design
//! invariant I2: apply <= committed vector point). The store additionally
//! persists the worker's committed point (`committed.vec`), the trust anchor
//! restart and leader-elect healing consult.
//!
//! The store is shared between the worker-local sink (append/flush), the
//! worker-local applier (read/promote/truncate) and the node-level repl
//! listener (heal-fetch reads) across threads; every method takes the
//! internal mutex only to plan or publish, never across an `.await`.

use ::wal::log_frame::{frame_lsn, split_frame};
use ::wal::lsn::LSN;
use ::wal::worker_log::scan_valid_frame_prefix;
use mudu::common::crc::crc64;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::sync::SMutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::warn;

/// Staging chunk size cap before rotating to the next chunk file.
const STAGING_CHUNK_SIZE: u64 = 16 * 1024 * 1024;

const STAGED_CHUNK_SUFFIX: &str = ".xls";
const COMMITTED_VEC_FILE_NAME: &str = "committed.vec";

/// Location and checksum of one staged frame.
#[derive(Clone, Copy, Debug)]
struct StagedLoc {
    chunk_seq: u64,
    offset: u64,
    len: u64,
    crc: u64,
}

#[derive(Default)]
struct StagingIndex {
    /// lsn -> staged frame location, in lsn order.
    frames: BTreeMap<u64, StagedLoc>,
    /// Current append tail (chunk seq + offset).
    tail: Option<(u64, u64)>,
    /// Next chunk sequence to create.
    next_seq: u64,
}

/// The committed point persisted per worker: the last `(lsn, seg_hash)` of
/// this worker's stream known to be raft-committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedCommittedPoint {
    /// Highest raft-committed LSN of this worker's stream.
    pub lsn: u64,
    /// Hash-chain value at `lsn`.
    pub hash: [u8; 32],
}

/// Follower staging store for one worker's streamed frames.
pub struct StagingStore {
    dir: PathBuf,
    fs: Arc<dyn AsyncFs>,
    index: SMutex<StagingIndex>,
}

impl StagingStore {
    /// Opens (creating if needed) the staging store in `dir`, truncating any
    /// torn tail left by a crash and rebuilding the frame index.
    pub async fn open(dir: PathBuf, fs: Arc<dyn AsyncFs>) -> RS<Self> {
        fs.create_dir_all(&dir).await?;
        let index = Self::scan_index(&dir, &fs).await?;
        Ok(Self {
            dir,
            fs,
            index: SMutex::new(index),
        })
    }

    /// Rebuilds the index from the chunk files on disk, truncating torn
    /// chunk tails left behind by a crash. Frames below the persisted
    /// committed point are not indexed: the prefix GC prunes the index and
    /// whole dead chunks, while the boundary chunk keeps its dead prefix on
    /// disk (bounded by one chunk) until it rotates out.
    async fn scan_index(dir: &Path, fs: &Arc<dyn AsyncFs>) -> RS<StagingIndex> {
        let committed_floor = Self::load_committed_point(dir)?.map(|point| point.lsn);
        let mut chunks: Vec<(u64, PathBuf)> = Vec::new();
        for path in fs.read_dir(dir).await? {
            if let Some(seq) = parse_chunk_seq(&path) {
                chunks.push((seq, path));
            }
        }
        chunks.sort_by_key(|(seq, _)| *seq);
        let mut index = StagingIndex::default();
        for (seq, path) in &chunks {
            let bytes = fs.read_all(path).await?;
            let prefix = scan_valid_frame_prefix(&bytes);
            if prefix.corrupt_reason.is_some() {
                warn!(
                    path = %path.display(),
                    "truncating un-persisted staging chunk tail"
                );
                truncate_file_to(path, prefix.valid_len as u64)?;
            }
            let mut offset = 0u64;
            for frame in &prefix.frames {
                let lsn = frame_lsn(frame)?.as_u64();
                if committed_floor.is_some_and(|floor| lsn < floor) {
                    offset += frame.len() as u64;
                    continue;
                }
                index.frames.insert(
                    lsn,
                    StagedLoc {
                        chunk_seq: *seq,
                        offset,
                        len: frame.len() as u64,
                        crc: crc64(frame),
                    },
                );
                offset += frame.len() as u64;
            }
            index.tail = Some((*seq, offset));
            index.next_seq = seq + 1;
        }
        Ok(index)
    }

    /// The staging directory of worker `worker_index` under the server's WAL
    /// log directory.
    pub fn dir_for_worker(log_dir: &str, worker_index: u32) -> PathBuf {
        PathBuf::from(log_dir)
            .join("repl_staging")
            .join(format!("worker_{worker_index}"))
    }

    /// The directory this store stages into.
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Highest staged LSN, if any.
    pub(crate) fn max_lsn(&self) -> RS<Option<LSN>> {
        let index = self.lock_index()?;
        Ok(index.frames.keys().next_back().map(|lsn| LSN::new(*lsn)))
    }

    /// CRC64 of the staged frame at `lsn`, when present.
    pub(crate) fn frame_crc(&self, lsn: u64) -> RS<Option<u64>> {
        Ok(self.lock_index()?.frames.get(&lsn).map(|loc| loc.crc))
    }

    /// Reads the staged frame bytes at `lsn`.
    pub(crate) async fn frame_bytes(&self, lsn: u64) -> RS<Option<Vec<u8>>> {
        let loc = self.lock_index()?.frames.get(&lsn).copied();
        let Some(loc) = loc else {
            return Ok(None);
        };
        let path = self.chunk_path(loc.chunk_seq);
        let file = self.fs.open(&path, FileOptions::read_only()).await?;
        let bytes = file.read_exact_at(loc.offset, loc.len as usize).await?;
        Ok(Some(bytes))
    }

    /// Appends frames at the tail (callers pass frames in strictly rising
    /// LSN order starting at `max_lsn + 1`). Not yet durable: call
    /// [`Self::flush`] before acknowledging the appended LSNs.
    pub(crate) async fn append_frames(&self, frames: &[Vec<u8>]) -> RS<()> {
        if frames.is_empty() {
            return Ok(());
        }
        // Plan the append under the lock; write outside it; publish after.
        let mut plan: Vec<(usize, u64, u64, u64, u64)> = Vec::with_capacity(frames.len());
        {
            let mut index = self.lock_index()?;
            let (mut seq, mut offset) = index.tail.unwrap_or((index.next_seq, 0));
            for (position, frame) in frames.iter().enumerate() {
                let lsn = frame_lsn(frame)?.as_u64();
                if index.frames.contains_key(&lsn) {
                    return Err(mudu_error!(
                        ErrorCode::InvalidState,
                        format!("staging append of duplicate lsn {lsn}")
                    ));
                }
                if offset >= STAGING_CHUNK_SIZE {
                    seq += 1;
                    offset = 0;
                }
                plan.push((position, lsn, seq, offset, crc64(frame)));
                offset += frame.len() as u64;
            }
            index.tail = Some((seq, offset));
            index.next_seq = index.next_seq.max(seq + 1);
        }
        for (position, _lsn, seq, offset, _crc) in &plan {
            let path = self.chunk_path(*seq);
            let file = self
                .fs
                .open(&path, FileOptions::read_write_create())
                .await?;
            file.write_all_at(*offset, &frames[*position]).await?;
        }
        let mut index = self.lock_index()?;
        for (position, lsn, seq, offset, crc) in plan {
            index.frames.insert(
                lsn,
                StagedLoc {
                    chunk_seq: seq,
                    offset,
                    len: frames[position].len() as u64,
                    crc,
                },
            );
        }
        Ok(())
    }

    /// fsyncs the current tail chunk: staged LSNs become durable (this is
    /// what a `ReplAck.stored_lsn` means).
    pub async fn flush(&self) -> RS<()> {
        let tail = self.lock_index()?.tail;
        let Some((seq, _)) = tail else {
            return Ok(());
        };
        let path = self.chunk_path(seq);
        let file = self
            .fs
            .open(&path, FileOptions::read_write_create())
            .await?;
        file.fsync().await
    }

    /// Drops every staged frame with `lsn < below_lsn` (the prefix mirror
    /// of [`Self::truncate_from`]): removes them from the index and deletes
    /// chunk files that hold no remaining frame. The append tail is never
    /// deleted; frames below `below_lsn` inside a kept chunk stay until it
    /// rotates out. Called as the committed chain advances. Returns the
    /// number of chunk files reclaimed.
    pub(crate) async fn gc_prefix(&self, below_lsn: u64) -> RS<u64> {
        let obsolete_seqs = {
            let mut index = self.lock_index()?;
            let removed: Vec<(u64, StagedLoc)> = index
                .frames
                .range(..below_lsn)
                .map(|(lsn, loc)| (*lsn, *loc))
                .collect();
            if removed.is_empty() {
                return Ok(0);
            }
            for (lsn, _) in &removed {
                index.frames.remove(lsn);
            }
            let mut remaining_counts: std::collections::HashMap<u64, usize> =
                std::collections::HashMap::new();
            for loc in index.frames.values() {
                *remaining_counts.entry(loc.chunk_seq).or_insert(0) += 1;
            }
            let removed_seqs: std::collections::HashSet<u64> =
                removed.iter().map(|(_, loc)| loc.chunk_seq).collect();
            let tail_seq = index.tail.map(|(seq, _)| seq);
            removed_seqs
                .into_iter()
                .filter(|seq| Some(*seq) != tail_seq && !remaining_counts.contains_key(seq))
                .collect::<Vec<u64>>()
        };
        let reclaimed = obsolete_seqs.len() as u64;
        for seq in obsolete_seqs {
            self.fs.remove_file_if_exists(&self.chunk_path(seq)).await?;
        }
        Ok(reclaimed)
    }

    /// Cuts every staged frame with `lsn >= from_lsn`: truncates the
    /// containing chunk at the first such frame and deletes later chunks.
    /// Used by divergence healing (the uncommitted staged tail is dropped).
    pub(crate) async fn truncate_from(&self, from_lsn: u64) -> RS<()> {
        let plan = {
            let index = self.lock_index()?;
            index.frames.range(from_lsn..).next().map(|(_, loc)| *loc)
        };
        let Some(cut) = plan else {
            return Ok(());
        };
        // Delete the chunks after the cut chunk, truncate the cut chunk at
        // the frame boundary, then rescan to rebuild the index and tail.
        let cut_path = self.chunk_path(cut.chunk_seq);
        truncate_file_to(&cut_path, cut.offset)?;
        let higher: Vec<PathBuf> = self
            .fs
            .read_dir(&self.dir)
            .await?
            .into_iter()
            .filter(|path| parse_chunk_seq(path).is_some_and(|seq| seq > cut.chunk_seq))
            .collect();
        for path in higher {
            self.fs.remove_file_if_exists(&path).await?;
        }
        let index = Self::scan_index(&self.dir, &self.fs).await?;
        *self.lock_index()? = index;
        Ok(())
    }

    /// All staged frames with `lo_lsn <= lsn < hi_lsn`, in LSN order
    /// (heal-fetch serving and promote reads).
    pub(crate) async fn frames_in_range(&self, lo_lsn: u64, hi_lsn: u64) -> RS<Vec<Vec<u8>>> {
        let locs: Vec<StagedLoc> = self
            .lock_index()?
            .frames
            .range(lo_lsn..hi_lsn)
            .map(|(_, loc)| *loc)
            .collect();
        let mut frames = Vec::with_capacity(locs.len());
        for loc in locs {
            let path = self.chunk_path(loc.chunk_seq);
            let file = self.fs.open(&path, FileOptions::read_only()).await?;
            frames.push(file.read_exact_at(loc.offset, loc.len as usize).await?);
        }
        Ok(frames)
    }

    /// Loads the persisted committed point of this worker, if present.
    pub fn load_committed_point(dir: &Path) -> RS<Option<PersistedCommittedPoint>> {
        let path = dir.join(COMMITTED_VEC_FILE_NAME);
        if !mudu_sys::fs::sync::sync_path_exists(&path) {
            return Ok(None);
        }
        let bytes = mudu_sys::fs::sync::read(&path)?;
        let point = rmp_serde::from_slice(&bytes).map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                format!("decode committed point file {} error", path.display()),
                e
            )
        })?;
        Ok(Some(point))
    }

    /// Persists the committed point (write + fsync): the anchor restart and
    /// leader-elect healing trust.
    pub(crate) fn store_committed_point(&self, point: &PersistedCommittedPoint) -> RS<()> {
        let path = self.dir.join(COMMITTED_VEC_FILE_NAME);
        let bytes = rmp_serde::to_vec(point)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode committed point error", e))?;
        let mut options = mudu_sys::fs::sync::SOpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut file = options.open(&path)?;
        file.write_all(&bytes)
            .map_err(|e| mudu_error!(ErrorCode::Io, "write committed point error", e))?;
        file.sync_data()?;
        Ok(())
    }

    fn chunk_path(&self, seq: u64) -> PathBuf {
        self.dir.join(format!("stg_{seq}{STAGED_CHUNK_SUFFIX}"))
    }

    fn lock_index(&self) -> RS<mudu_sys::sync::SMutexGuard<'_, StagingIndex>> {
        self.index
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "staging index lock poisoned"))
    }
}

fn parse_chunk_seq(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let seq = name
        .strip_prefix("stg_")?
        .strip_suffix(STAGED_CHUNK_SUFFIX)?;
    seq.parse::<u64>().ok()
}

/// Truncates `path` to `len` bytes and persists the size change (same
/// pattern as the WAL layout's torn-tail truncation).
fn truncate_file_to(path: &Path, len: u64) -> RS<()> {
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.read(true).write(true);
    let file = options.open(path)?;
    file.set_len(len)?;
    file.sync_data()?;
    Ok(())
}

/// Validates a frame's structure and returns its LSN (used by the sink's
/// divergence byte-compare and heal paths).
pub(crate) fn staged_frame_lsn(frame: &[u8]) -> RS<LSN> {
    Ok(split_frame(frame)?.0.lsn())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use ::wal::log_frame::serialize_entry;
    use mudu_sys::default_sys_io_context;
    use std::sync::atomic::AtomicU64;

    fn make_frames(start_lsn: u64, count: u64) -> Vec<Vec<u8>> {
        let next = AtomicU64::new(start_lsn);
        let mut out = Vec::new();
        for i in 0..count {
            let parts = serialize_entry(&format!("value-{i}"), 4096, &next).unwrap();
            out.extend(parts);
        }
        out
    }

    fn test_fs() -> Arc<dyn AsyncFs> {
        default_sys_io_context().provider_arc().fs_arc()
    }

    fn reopened_store_point(store: &StagingStore, lsn: u64) {
        store
            .store_committed_point(&PersistedCommittedPoint {
                lsn,
                hash: [1u8; 32],
            })
            .unwrap();
    }

    #[test]
    fn staging_roundtrip_truncate_and_reopen() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let dir = mudu_sys::env_var::temp_dir()
                .join(format!("staging-test-{}", mudu_sys::random::uuid_v4()));
            let store = StagingStore::open(dir.clone(), test_fs()).await.unwrap();
            let frames = make_frames(0, 6);
            store.append_frames(&frames).await.unwrap();
            store.flush().await.unwrap();
            assert_eq!(store.max_lsn().unwrap(), Some(LSN::new(5)));
            assert_eq!(
                store.frame_bytes(3).await.unwrap().as_deref(),
                Some(frames[3].as_slice())
            );
            assert_eq!(store.frame_crc(3).unwrap(), Some(crc64(&frames[3])));

            store.truncate_from(4).await.unwrap();
            assert_eq!(store.max_lsn().unwrap(), Some(LSN::new(3)));
            assert!(store.frame_bytes(4).await.unwrap().is_none());

            // reopen sees the truncated state
            let reopened = StagingStore::open(dir.clone(), test_fs()).await.unwrap();
            assert_eq!(reopened.max_lsn().unwrap(), Some(LSN::new(3)));

            // appending again after a truncate continues at the cut point
            let more = make_frames(4, 2);
            reopened.append_frames(&more).await.unwrap();
            reopened.flush().await.unwrap();
            assert_eq!(reopened.max_lsn().unwrap(), Some(LSN::new(5)));
            assert_eq!(
                reopened.frame_bytes(4).await.unwrap().as_deref(),
                Some(more[0].as_slice())
            );

            // committed point persists across reopen
            let point = PersistedCommittedPoint {
                lsn: 3,
                hash: [9u8; 32],
            };
            reopened.store_committed_point(&point).unwrap();
            assert_eq!(
                StagingStore::load_committed_point(&dir).unwrap(),
                Some(point)
            );
            Ok::<_, mudu::error::MuduError>(())
        })
        .unwrap()
        .unwrap()
    }

    #[test]
    fn staging_gc_prefix_drops_committed_frames() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let dir = mudu_sys::env_var::temp_dir()
                .join(format!("staging-gc-test-{}", mudu_sys::random::uuid_v4()));
            let store = StagingStore::open(dir.clone(), test_fs()).await.unwrap();
            let frames = make_frames(0, 6);
            store.append_frames(&frames).await.unwrap();
            store.flush().await.unwrap();

            // below the chain point: gone from the index, later frames stay
            // (the applier persists the committed point before the prefix GC)
            reopened_store_point(&store, 4);
            store.gc_prefix(4).await.unwrap();
            assert!(store.frame_bytes(3).await.unwrap().is_none());
            assert!(store.frame_crc(2).unwrap().is_none());
            assert_eq!(
                store.frame_bytes(4).await.unwrap().as_deref(),
                Some(frames[4].as_slice())
            );
            assert_eq!(store.max_lsn().unwrap(), Some(LSN::new(5)));

            // reopen does not resurrect the dropped prefix
            let reopened = StagingStore::open(dir.clone(), test_fs()).await.unwrap();
            assert!(reopened.frame_bytes(3).await.unwrap().is_none());
            assert_eq!(reopened.max_lsn().unwrap(), Some(LSN::new(5)));

            // appends after a prefix GC continue at the tail
            let more = make_frames(6, 1);
            reopened.append_frames(&more).await.unwrap();
            reopened.flush().await.unwrap();
            assert_eq!(reopened.max_lsn().unwrap(), Some(LSN::new(6)));

            // gc everything staged: the tail chunk stays usable
            reopened.gc_prefix(7).await.unwrap();
            assert!(reopened.frame_bytes(6).await.unwrap().is_none());
            let even_more = make_frames(7, 1);
            reopened.append_frames(&even_more).await.unwrap();
            reopened.flush().await.unwrap();
            assert_eq!(reopened.max_lsn().unwrap(), Some(LSN::new(7)));
            Ok::<_, mudu::error::MuduError>(())
        })
        .unwrap()
        .unwrap()
    }
}
