//! Persistent raft log and hard-state store.
//!
//! Layout (all under `<log_dir>/raft_log/`):
//!
//! - one ordered series of chunk files `<seq:020>.rlc`;
//! - each chunk holds frames in the MuduDB log frame format v1
//!   (see [`crate::frame`], byte-compatible with the kernel WAL);
//! - every frame series encodes an ordered stream of `StoreStruct`s:
//!   log `Entry` records, `TruncateRight`/`TruncateLeft` markers and
//!   `State` snapshots (term/vote, commit index, configurations, snapshot
//!   frontier).
//!
//! Design notes:
//!
//! - A single ordered series (rather than separate entry/state files) makes
//!   every batch an append of a contiguous frame run followed by one fsync,
//!   so a crash can only leave a *prefix* of the batch behind — never a
//!   state that is ahead of the log or vice versa.
//! - The frame header `lsn` carries the **raft log index** for `Entry`
//!   records (the raft index *is* the position counter) and `0` for other
//!   record kinds.
//! - One logical record never straddles two chunk files: the writer rotates
//!   to a fresh chunk when the next record does not fit. Oversize records
//!   are split into multi-part frames *within* one chunk.
//! - Recovery scans chunks in sequence order, keeps the longest valid frame
//!   prefix, physically truncates the first torn chunk to its valid prefix
//!   and deletes all later chunks, then replays the records in order.
//!   Committed entries are never lost this way (a torn tail only removes a
//!   not-yet-acknowledged suffix; duplicates after re-commitment are
//!   possible — the committed stream is at-least-once).
//! - Log truncation uses `TruncateRight` marker frames instead of rewriting
//!   files, so appends are always sequential.
//! - Physical garbage collection: when the snapshot frontier advances
//!   (`CompactLog` / `InstallSnapshot`), whole chunks whose entries all
//!   lie at or below the frontier are deleted after the batch is durable.
//!   If the chunk holding the latest `State` record is itself eligible,
//!   the state is first replayed into a fresh chunk and fsynced. The
//!   current tail chunk is never deleted; its dead bytes are reclaimed
//!   when rotation turns it into an old chunk.

use mudu_sys::contract::{async_file::AsyncFile, async_fs::AsyncFs, file_options::FileOptions};
mod async_store;
/// Injectable asynchronous persistence boundary and its operation future.
pub use async_store::{AsyncStore, StoreFuture};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
#[cfg(test)]
use mudu_sys::fs::sync as fs_sync;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::core::conf::{ConfNodeValue, ConfValue, ConfVersion};
use crate::core::message::{LogEntry, TermIndex, WriteOp};
use crate::core::CoreRecovered;
use crate::frame::{reassemble_records, scan_valid_frame_prefix, serialize_record};

/// Directory name of the raft log inside the configured log directory.
pub const RAFT_LOG_DIR_NAME: &str = "raft_log";

const CHUNK_SUFFIX: &str = ".rlc";

/// Default chunk size limit (16 MiB) before rotation.
pub const DEFAULT_CHUNK_SIZE: u64 = 16 * 1024 * 1024;

/// Default maximum size of one frame part (1 MiB).
pub const DEFAULT_MAX_PART_SIZE: usize = 1024 * 1024;

/// The raft log directory for a configured log directory.
pub fn raft_log_dir(log_dir: &Path) -> PathBuf {
    log_dir.join(RAFT_LOG_DIR_NAME)
}

/// Store configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StoreCfg {
    /// Chunk file size limit in bytes; rotation happens when the next
    /// record would exceed it.
    pub chunk_size: u64,
    /// Maximum size of one frame part in bytes.
    pub max_part_size: usize,
    /// Whether to fsync after every applied batch.
    pub fsync: bool,
}

impl Default for StoreCfg {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            max_part_size: DEFAULT_MAX_PART_SIZE,
            fsync: true,
        }
    }
}

/// Persisted hard state, rewritten as a whole in every `State` record.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
struct StateStruct {
    current_term: u64,
    voted_for: Option<u64>,
    commit_index: u64,
    conf_committed: Option<ConfNodeValue>,
    conf_new: Option<ConfNodeValue>,
    snapshot_index: u64,
    snapshot_term: u64,
}

/// One record in the ordered store stream.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
enum StoreStruct {
    /// One raft log entry.
    Entry(LogEntry),
    /// Drop all persisted entries with index `>= from_index`.
    TruncateRight { from_index: u64 },
    /// Drop all persisted entries with index `<= to_index` (reserved).
    TruncateLeft { to_index: u64 },
    /// Full hard-state snapshot.
    State(Box<StateStruct>),
}

/// The persistent raft store. Single-writer, owned by the asynchronous driver.
pub struct RaftStore {
    dir: PathBuf,
    cfg: StoreCfg,
    state: StateStruct,
    last_entry_index: u64,
    tail_seq: u64,
    tail_offset: u64,
    tail_file: Option<Arc<dyn AsyncFile>>,
    fs: Arc<dyn AsyncFs>,
    dir_dirty: bool,
    /// Per-chunk maximum entry index it contains (0 = no entries); used to
    /// decide which whole chunks fall entirely below the snapshot
    /// frontier. Contains every known chunk, including the tail.
    chunk_max: BTreeMap<u64, u64>,
    /// Sequence number of the chunk holding the latest `State` record.
    /// That chunk must survive garbage collection; when it becomes
    /// eligible for deletion, the latest state is first replayed into a
    /// fresh chunk.
    state_chunk_seq: u64,
}

impl RaftStore {
    /// Opens and repairs a store using Tokio-backed asynchronous IO.
    /// Recovery must finish before the driver accepts protocol events.
    pub async fn open_async(dir: &Path, cfg: StoreCfg) -> RS<(Self, Option<CoreRecovered>)> {
        let fs = mudu_sys::fs::tokio_fs();
        if cfg.chunk_size == 0 || cfg.max_part_size == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "raft store chunk_size and max_part_size must be non-zero"
            ));
        }
        ensure_store_dir(fs.as_ref(), dir).await?;
        let chunks = list_chunks(fs.as_ref(), dir).await?;
        let mut scan = scan_chunks(fs.as_ref(), &chunks).await?;
        repair_torn(fs.as_ref(), &chunks, &mut scan).await?;
        fs.sync_dir(dir).await?;
        let snapshot_index = scan.state.snapshot_index;
        let log = build_log(snapshot_index, &mut scan.entries)?;
        let last_entry_index = log.last().map(|e| e.index).unwrap_or(snapshot_index);
        let recovered = build_recovered(&scan.state, log, last_entry_index, scan.state_seen)?;
        let (tail_seq, tail_offset, tail_file) = open_tail(fs.as_ref(), dir, scan.tail).await?;
        debug!(
            "raft store opened at {:?}: {} entries recovered, tail seq {} offset {}",
            dir,
            recovered.as_ref().map_or(0, |r| r.log.len()),
            tail_seq,
            tail_offset
        );
        Ok((
            Self {
                dir: dir.to_path_buf(),
                fs,
                dir_dirty: false,
                cfg,
                state: scan.state,
                last_entry_index,
                tail_seq,
                tail_offset,
                tail_file,
                chunk_max: scan.chunk_max,
                state_chunk_seq: scan.state_chunk_seq,
            },
            recovered,
        ))
    }

    /// Applies one batch of write operations and (by default) fsyncs.
    ///
    /// Within a batch, entry records are appended first and the state
    /// record last, so a crash can never persist a state ahead of the log.
    ///
    /// When the batch advanced the snapshot frontier (`CompactLog` /
    /// `InstallSnapshot`), whole chunks that lie entirely below the new
    /// frontier are physically reclaimed after the batch is durable
    /// (lazy GC; crash-safe at any point).
    /// Appends one batch and awaits its durable boundary before returning.
    /// The driver must not cancel this future or reuse the store after an IO error.
    pub async fn apply_batch_async(&mut self, ops: &[WriteOp]) -> RS<()> {
        let (records, snapshot_advanced) = self.translate_ops(ops);
        if records.is_empty() {
            return Ok(());
        }
        let mut rotated: Vec<Arc<dyn AsyncFile>> = Vec::new();
        for record in &records {
            self.append_record(record, &mut rotated).await?;
        }
        if self.cfg.fsync {
            for file in &rotated {
                file.fsync().await?;
            }
            if let Some(file) = &self.tail_file {
                file.fsync().await?;
            }
        }
        if self.cfg.fsync && self.dir_dirty {
            self.fs.sync_dir(&self.dir).await?;
            self.dir_dirty = false;
        }
        if snapshot_advanced && self.cfg.fsync {
            self.gc_compact_chunks().await?;
        }
        Ok(())
    }

    /// Translates one batch of write operations into the ordered record
    /// stream to append, updating the in-memory state as it goes;
    /// returns the records and whether the snapshot frontier advanced.
    /// The state record (when dirty) is pushed last.
    fn translate_ops(&mut self, ops: &[WriteOp]) -> (Vec<StoreStruct>, bool) {
        let mut records: Vec<StoreStruct> = Vec::new();
        let mut state_dirty = false;
        let mut snapshot_advanced = false;
        for op in ops {
            match op {
                WriteOp::WriteLog {
                    prev_index,
                    entries,
                    opt,
                } => {
                    if opt.truncate_left {
                        records.push(StoreStruct::TruncateLeft {
                            to_index: *prev_index,
                        });
                    }
                    if opt.truncate_right && *prev_index < self.last_entry_index {
                        records.push(StoreStruct::TruncateRight {
                            from_index: prev_index + 1,
                        });
                        self.last_entry_index = *prev_index;
                    }
                    for entry in entries {
                        if entry.index <= self.state.snapshot_index {
                            // already compacted; defensive skip
                            continue;
                        }
                        records.push(StoreStruct::Entry(entry.clone()));
                        self.last_entry_index = entry.index;
                    }
                }
                WriteOp::UpTermVotedFor { term, voted_for } => {
                    self.state.current_term = *term;
                    self.state.voted_for = *voted_for;
                    state_dirty = true;
                }
                WriteOp::UpCommitIndex(commit_index) => {
                    self.state.commit_index = *commit_index;
                    state_dirty = true;
                }
                WriteOp::UpConfCommitted { value, version } => {
                    self.state.conf_committed = Some(conf_from_value_version(value, version));
                    state_dirty = true;
                }
                WriteOp::UpConfNew { value, version } => {
                    self.state.conf_new = Some(conf_from_value_version(value, version));
                    state_dirty = true;
                }
                WriteOp::CompactLog(entries) => {
                    if let Some(last) = entries.last() {
                        if last.index > self.state.snapshot_index {
                            snapshot_advanced = true;
                        }
                        self.state.snapshot_index = last.index;
                        self.state.snapshot_term = last.term;
                        state_dirty = true;
                    }
                }
                WriteOp::InstallSnapshot(frontier) => {
                    if frontier.index > self.state.snapshot_index {
                        snapshot_advanced = true;
                    }
                    self.state.snapshot_index = frontier.index;
                    self.state.snapshot_term = frontier.term;
                    state_dirty = true;
                }
            }
        }
        if state_dirty {
            records.push(StoreStruct::State(Box::new(self.state.clone())));
        }
        (records, snapshot_advanced)
    }

    /// Waits for tail data and directory entries to become durable.
    pub async fn flush_async(&mut self) -> RS<()> {
        if let Some(file) = &self.tail_file {
            file.fsync().await?;
        }
        if self.dir_dirty {
            self.fs.sync_dir(&self.dir).await?;
            self.dir_dirty = false;
        }
        Ok(())
    }

    /// The store directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The persisted snapshot frontier `(index, term)`.
    pub fn snapshot_frontier(&self) -> (u64, u64) {
        (self.state.snapshot_index, self.state.snapshot_term)
    }

    async fn append_record(
        &mut self,
        record: &StoreStruct,
        rotated: &mut Vec<Arc<dyn AsyncFile>>,
    ) -> RS<()> {
        let payload = rmp_serde::to_vec(record)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode raft store record error", e))?;
        let lsn = match record {
            StoreStruct::Entry(entry) => entry.index,
            _ => 0,
        };
        let bytes = serialize_record(&payload, lsn, self.cfg.max_part_size)?;
        if self.tail_file.is_none() {
            let path = chunk_path(&self.dir, self.tail_seq);
            let file = create_chunk_file(self.fs.as_ref(), &path).await?;
            self.dir_dirty = true;
            self.tail_file = Some(file);
            self.tail_offset = 0;
            self.chunk_max.insert(self.tail_seq, 0);
        }
        if self.tail_offset > 0 && self.tail_offset + bytes.len() as u64 > self.cfg.chunk_size {
            // rotate: the record lands entirely in a fresh chunk
            self.rotate_tail(rotated).await?;
        }
        let file = self
            .tail_file
            .as_mut()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "raft store tail file is not open"))?;
        file.write_all_at(self.tail_offset, &bytes).await?;
        self.tail_offset += bytes.len() as u64;
        match record {
            StoreStruct::Entry(entry) => {
                self.chunk_max.insert(self.tail_seq, entry.index);
            }
            StoreStruct::State(_) => {
                self.state_chunk_seq = self.tail_seq;
            }
            _ => {}
        }
        Ok(())
    }

    /// Rotates to a fresh tail chunk (the previous tail is queued for
    /// fsync in `rotated`).
    async fn rotate_tail(&mut self, rotated: &mut Vec<Arc<dyn AsyncFile>>) -> RS<()> {
        self.tail_seq += 1;
        let path = chunk_path(&self.dir, self.tail_seq);
        let file = create_chunk_file(self.fs.as_ref(), &path).await?;
        self.dir_dirty = true;
        if let Some(old) = self.tail_file.replace(file) {
            rotated.push(old);
        }
        self.tail_offset = 0;
        self.chunk_max.insert(self.tail_seq, 0);
        Ok(())
    }

    /// Physically deletes whole chunks whose entries all lie at or below
    /// the snapshot frontier.
    ///
    /// Crash safety: if the chunk holding the latest `State` record is
    /// itself eligible for deletion, the state is first replayed into a
    /// fresh chunk and fsynced, so the retained series always carries a
    /// valid state. The deletion itself needs no fsync: a crash anywhere
    /// simply leaves some old chunks behind, and the next frontier
    /// advance retries the reclamation. The current tail is never
    /// deleted; its dead bytes are reclaimed once rotation turns it into
    /// an old chunk.
    async fn gc_compact_chunks(&mut self) -> RS<()> {
        let frontier = self.state.snapshot_index;
        if frontier == 0 {
            return Ok(());
        }
        // the latest State must live in a retained chunk
        let state_chunk_retained = self.state_chunk_seq == self.tail_seq
            || self
                .chunk_max
                .get(&self.state_chunk_seq)
                .copied()
                .unwrap_or(0)
                > frontier;
        if !state_chunk_retained {
            debug!(
                "raft store: replaying the latest state into a fresh chunk before GC (frontier {})",
                frontier
            );
            let mut rotated = Vec::new();
            self.rotate_tail(&mut rotated).await?;
            let record = StoreStruct::State(Box::new(self.state.clone()));
            self.append_record(&record, &mut rotated).await?;
            if self.cfg.fsync {
                for file in &rotated {
                    file.fsync().await?;
                }
                if let Some(file) = &self.tail_file {
                    file.fsync().await?;
                }
            }
        }
        if self.dir_dirty {
            self.fs.sync_dir(&self.dir).await?;
            self.dir_dirty = false;
        }
        let deletable: Vec<u64> = self
            .chunk_max
            .iter()
            .filter(|(seq, max_index)| {
                **seq != self.tail_seq && **seq != self.state_chunk_seq && **max_index <= frontier
            })
            .map(|(seq, _)| *seq)
            .collect();
        for seq in deletable {
            debug!(
                "raft store: deleting chunk {} (entirely below snapshot frontier {})",
                seq, frontier
            );
            self.fs
                .remove_file_if_exists(&chunk_path(&self.dir, seq))
                .await?;
            self.chunk_max.remove(&seq);
            self.dir_dirty = true;
        }
        Ok(())
    }
}

/// The replayed content of the chunk series after a scan: the recovered
/// state record, the surviving entries, the append tail and the
/// physical-repair bookkeeping.
struct ScanReplay {
    state: StateStruct,
    entries: BTreeMap<u64, LogEntry>,
    state_seen: bool,
    /// The last fully valid chunk `(seq, valid length)`; the append tail.
    tail: Option<(u64, u64)>,
    /// The first torn chunk `(chunk list index, valid prefix length)`.
    torn: Option<(usize, u64)>,
    chunk_max: BTreeMap<u64, u64>,
    state_chunk_seq: u64,
}

/// Scans the chunk series in sequence order and replays every record,
/// stopping at the first torn chunk.
async fn scan_chunks(fs: &dyn AsyncFs, chunks: &[(u64, PathBuf)]) -> RS<ScanReplay> {
    let mut state = StateStruct::default();
    let mut entries: BTreeMap<u64, LogEntry> = BTreeMap::new();
    let mut state_seen = false;
    let mut tail: Option<(u64, u64)> = None;
    let mut torn: Option<(usize, u64)> = None;
    let mut chunk_max: BTreeMap<u64, u64> = BTreeMap::new();
    let mut state_chunk_seq = 0u64;

    for (i, (seq, path)) in chunks.iter().enumerate() {
        let buf = fs.read_all(path).await?;
        let scanned = scan_valid_frame_prefix(&buf);
        let records = reassemble_records(&scanned.frames)?;
        chunk_max.insert(*seq, 0);
        for (_lsn, payload) in records {
            let record: StoreStruct = rmp_serde::from_slice(&payload)
                .map_err(|e| mudu_error!(ErrorCode::Decode, "decode raft store record error", e))?;
            match &record {
                StoreStruct::Entry(entry) => {
                    chunk_max.insert(*seq, entry.index);
                }
                StoreStruct::State(_) => {
                    state_chunk_seq = *seq;
                }
                _ => {}
            }
            apply_replay(record, &mut state, &mut entries, &mut state_seen);
        }
        if scanned.valid_len < buf.len() {
            warn!(
                "raft store chunk {:?} has a torn tail at byte {}",
                path, scanned.valid_len
            );
            torn = Some((i, scanned.valid_len as u64));
            break;
        }
        tail = Some((*seq, buf.len() as u64));
    }
    Ok(ScanReplay {
        state,
        entries,
        state_seen,
        tail,
        torn,
        chunk_max,
        state_chunk_seq,
    })
}

/// Physically repairs the series after a torn scan: truncates the torn
/// chunk and drops later chunks (their records form no valid prefix
/// continuation).
async fn repair_torn(fs: &dyn AsyncFs, chunks: &[(u64, PathBuf)], scan: &mut ScanReplay) -> RS<()> {
    if let Some((i, valid_len)) = scan.torn {
        let (seq, path) = &chunks[i];
        for (_, later) in chunks.iter().skip(i + 1) {
            fs.remove_file_if_exists(later).await?;
        }
        if valid_len == 0 && i > 0 {
            fs.remove_file_if_exists(path).await?;
            // tail stays at the previous chunk (set during the scan)
            scan.chunk_max.remove(seq);
            if scan.state_chunk_seq == *seq {
                scan.state_chunk_seq = 0;
            }
        } else {
            let file = open_chunk_file(fs, path, false).await?;
            file.set_len(valid_len).await?;
            file.fsync().await?;
            scan.tail = Some((*seq, valid_len));
        }
    }
    Ok(())
}

/// Drops compaction residue and validates log contiguity from the
/// snapshot frontier, returning the ordered surviving log.
fn build_log(snapshot_index: u64, entries: &mut BTreeMap<u64, LogEntry>) -> RS<Vec<LogEntry>> {
    entries.retain(|idx, _| *idx > snapshot_index);
    let mut log = Vec::with_capacity(entries.len());
    for (expect, (idx, entry)) in (snapshot_index + 1..).zip(entries.iter()) {
        if *idx != expect {
            if log.is_empty() {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "raft log does not start at the snapshot frontier: snapshot index {}, first entry index {}",
                        snapshot_index, idx
                    )
                ));
            }
            warn!(
                "raft store: gap at log index {}; dropping the remaining tail",
                expect
            );
            break;
        }
        log.push(entry.clone());
    }
    Ok(log)
}

/// Builds the recovered core state from the replayed state record and
/// the surviving log; `None` for a fresh bootstrap (no state record).
fn build_recovered(
    state: &StateStruct,
    log: Vec<LogEntry>,
    last_entry_index: u64,
    state_seen: bool,
) -> RS<Option<CoreRecovered>> {
    let recovered = if state_seen {
        let conf_committed = state.conf_committed.clone().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "raft store has state but no committed configuration"
            )
        })?;
        let conf_new = state.conf_new.clone().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "raft store has state but no new configuration"
            )
        })?;
        let commit_index = state
            .commit_index
            .max(state.snapshot_index)
            .min(last_entry_index);
        Some(CoreRecovered {
            conf_committed,
            conf_new,
            log,
            snapshot_index_term: TermIndex {
                term: state.snapshot_term,
                index: state.snapshot_index,
            },
            term: state.current_term,
            voted_for: state.voted_for,
            commit_index,
        })
    } else {
        if !log.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "raft store has log entries but no state record"
            ));
        }
        None
    };
    Ok(recovered)
}

/// Opens the tail chunk for appending at its valid-length offset, or
/// starts a fresh series at sequence 1 when no chunk survived.
async fn open_tail(
    fs: &dyn AsyncFs,
    dir: &Path,
    tail: Option<(u64, u64)>,
) -> RS<(u64, u64, Option<Arc<dyn AsyncFile>>)> {
    match tail {
        Some((seq, offset)) => {
            let file = open_chunk_file(fs, &chunk_path(dir, seq), false).await?;
            Ok((seq, offset, Some(file)))
        }
        None => Ok((1, 0, None)),
    }
}

fn conf_from_value_version(value: &ConfValue, version: &ConfVersion) -> ConfNodeValue {
    ConfNodeValue::new(value.clone(), version.term, version.version, version.index)
}

fn apply_replay(
    record: StoreStruct,
    state: &mut StateStruct,
    entries: &mut BTreeMap<u64, LogEntry>,
    state_seen: &mut bool,
) {
    match record {
        StoreStruct::Entry(entry) => {
            entries.insert(entry.index, entry);
        }
        StoreStruct::TruncateRight { from_index } => {
            entries.retain(|idx, _| *idx < from_index);
        }
        StoreStruct::TruncateLeft { to_index } => {
            entries.retain(|idx, _| *idx > to_index);
        }
        StoreStruct::State(record) => {
            *state = *record;
            *state_seen = true;
        }
    }
}

fn chunk_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("{seq:020}{CHUNK_SUFFIX}"))
}

async fn list_chunks(fs: &dyn AsyncFs, dir: &Path) -> RS<Vec<(u64, PathBuf)>> {
    let mut chunks = Vec::new();
    for path in fs.read_dir(dir).await? {
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        if let Some(stem) = name.strip_suffix(CHUNK_SUFFIX) {
            if let Ok(seq) = stem.parse::<u64>() {
                chunks.push((seq, path));
            }
        }
    }
    chunks.sort_by_key(|(seq, _)| *seq);
    Ok(chunks)
}

async fn ensure_store_dir(fs: &dyn AsyncFs, dir: &Path) -> RS<()> {
    let mut missing = Vec::new();
    let mut ancestor = dir;
    while !fs.path_exists(ancestor).await? {
        missing.push(ancestor.to_path_buf());
        ancestor = ancestor
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
    }
    fs.create_dir_all(dir).await?;
    // Persist newly created entries from the deepest directory back to
    // the first pre-existing ancestor, including a newly created log_dir.
    for path in &missing {
        fs.sync_dir(path).await?;
    }
    if !missing.is_empty() {
        fs.sync_dir(ancestor).await?;
    }
    Ok(())
}

async fn open_chunk_file(fs: &dyn AsyncFs, path: &Path, create: bool) -> RS<Arc<dyn AsyncFile>> {
    fs.open(
        path,
        FileOptions {
            read: true,
            write: true,
            create,
            ..FileOptions::default()
        },
    )
    .await
}

async fn create_chunk_file(fs: &dyn AsyncFs, path: &Path) -> RS<Arc<dyn AsyncFile>> {
    let options = FileOptions {
        read: true,
        write: true,
        create_new: true,
        ..FileOptions::default()
    };
    match fs.open(path, options).await {
        Ok(file) => Ok(file),
        Err(e) if e.ec() == ErrorCode::AlreadyExists => {
            let file = open_chunk_file(fs, path, false).await?;
            file.set_len(0).await?;
            Ok(file)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests;
