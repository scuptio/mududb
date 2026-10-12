//! Persistent raft log and hard-state store.
//!
//! Layout (all under `<log_dir>/raft_log/`):
//!
//! - one ordered series of chunk files `<seq:020>.rlc`;
//! - each chunk holds frames in the MuduDB log frame format v1
//!   (see [`crate::frame`], byte-compatible with the kernel WAL);
//! - every frame series encodes an ordered stream of [`StoreRecord`]s:
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

use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
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
struct StateRecord {
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
enum StoreRecord {
    /// One raft log entry.
    Entry(LogEntry),
    /// Drop all persisted entries with index `>= from_index`.
    TruncateRight { from_index: u64 },
    /// Drop all persisted entries with index `<= to_index` (reserved).
    TruncateLeft { to_index: u64 },
    /// Full hard-state snapshot.
    State(Box<StateRecord>),
}

/// The persistent raft store. Single-writer (owned by the persist thread).
pub struct RaftStore {
    dir: PathBuf,
    cfg: StoreCfg,
    state: StateRecord,
    last_entry_index: u64,
    tail_seq: u64,
    tail_offset: u64,
    tail_file: Option<fs_sync::SFile>,
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
    /// Opens (creating if necessary) the store under `dir`.
    ///
    /// Returns the store plus the recovered state, or `None` when the store
    /// is empty (fresh bootstrap).
    pub fn open(dir: &Path, cfg: StoreCfg) -> RS<(Self, Option<CoreRecovered>)> {
        if cfg.chunk_size == 0 || cfg.max_part_size == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "raft store chunk_size and max_part_size must be non-zero"
            ));
        }
        fs_sync::create_dir_all(dir)?;
        let chunks = list_chunks(dir)?;
        let mut scan = scan_chunks(&chunks)?;
        repair_torn(&chunks, &mut scan)?;
        let snapshot_index = scan.state.snapshot_index;
        let log = build_log(snapshot_index, &mut scan.entries)?;
        let last_entry_index = log.last().map(|e| e.index).unwrap_or(snapshot_index);
        let recovered = build_recovered(&scan.state, log, last_entry_index, scan.state_seen)?;
        let (tail_seq, tail_offset, tail_file) = open_tail(dir, scan.tail)?;
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
    pub fn apply_batch(&mut self, ops: &[WriteOp]) -> RS<()> {
        let (records, snapshot_advanced) = self.translate_ops(ops);
        if records.is_empty() {
            return Ok(());
        }
        let mut rotated: Vec<fs_sync::SFile> = Vec::new();
        for record in &records {
            self.append_record(record, &mut rotated)?;
        }
        if self.cfg.fsync {
            for file in &rotated {
                file.sync_data()?;
            }
            if let Some(file) = &self.tail_file {
                file.sync_data()?;
            }
        }
        if snapshot_advanced {
            self.gc_compact_chunks()?;
        }
        Ok(())
    }

    /// Translates one batch of write operations into the ordered record
    /// stream to append, updating the in-memory state as it goes;
    /// returns the records and whether the snapshot frontier advanced.
    /// The state record (when dirty) is pushed last.
    fn translate_ops(&mut self, ops: &[WriteOp]) -> (Vec<StoreRecord>, bool) {
        let mut records: Vec<StoreRecord> = Vec::new();
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
                        records.push(StoreRecord::TruncateLeft {
                            to_index: *prev_index,
                        });
                    }
                    if opt.truncate_right && *prev_index < self.last_entry_index {
                        records.push(StoreRecord::TruncateRight {
                            from_index: prev_index + 1,
                        });
                        self.last_entry_index = *prev_index;
                    }
                    for entry in entries {
                        if entry.index <= self.state.snapshot_index {
                            // already compacted; defensive skip
                            continue;
                        }
                        records.push(StoreRecord::Entry(entry.clone()));
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
            records.push(StoreRecord::State(Box::new(self.state.clone())));
        }
        (records, snapshot_advanced)
    }

    /// Flushes the store (fsync of the tail chunk).
    pub fn flush(&mut self) -> RS<()> {
        if let Some(file) = &self.tail_file {
            file.sync_data()?;
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

    fn append_record(&mut self, record: &StoreRecord, rotated: &mut Vec<fs_sync::SFile>) -> RS<()> {
        let payload = rmp_serde::to_vec(record)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode raft store record error", e))?;
        let lsn = match record {
            StoreRecord::Entry(entry) => entry.index,
            _ => 0,
        };
        let bytes = serialize_record(&payload, lsn, self.cfg.max_part_size)?;
        if self.tail_file.is_none() {
            let path = chunk_path(&self.dir, self.tail_seq);
            let file = create_chunk_file(&path)?;
            self.tail_file = Some(file);
            self.tail_offset = 0;
            self.chunk_max.insert(self.tail_seq, 0);
        }
        if self.tail_offset > 0 && self.tail_offset + bytes.len() as u64 > self.cfg.chunk_size {
            // rotate: the record lands entirely in a fresh chunk
            self.rotate_tail(rotated)?;
        }
        let file = self
            .tail_file
            .as_mut()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "raft store tail file is not open"))?;
        file.write_all(&bytes)
            .map_err(|e| mudu_error!(ErrorCode::Io, "write raft chunk error", e))?;
        self.tail_offset += bytes.len() as u64;
        match record {
            StoreRecord::Entry(entry) => {
                self.chunk_max.insert(self.tail_seq, entry.index);
            }
            StoreRecord::State(_) => {
                self.state_chunk_seq = self.tail_seq;
            }
            _ => {}
        }
        Ok(())
    }

    /// Rotates to a fresh tail chunk (the previous tail is queued for
    /// fsync in `rotated`).
    fn rotate_tail(&mut self, rotated: &mut Vec<fs_sync::SFile>) -> RS<()> {
        self.tail_seq += 1;
        let path = chunk_path(&self.dir, self.tail_seq);
        let file = create_chunk_file(&path)?;
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
    fn gc_compact_chunks(&mut self) -> RS<()> {
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
            self.rotate_tail(&mut rotated)?;
            let record = StoreRecord::State(Box::new(self.state.clone()));
            self.append_record(&record, &mut rotated)?;
            if self.cfg.fsync {
                for file in &rotated {
                    file.sync_data()?;
                }
                if let Some(file) = &self.tail_file {
                    file.sync_data()?;
                }
            }
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
            fs_sync::remove_file(chunk_path(&self.dir, seq))?;
            self.chunk_max.remove(&seq);
        }
        Ok(())
    }
}

/// The replayed content of the chunk series after a scan: the recovered
/// state record, the surviving entries, the append tail and the
/// physical-repair bookkeeping.
struct ScanReplay {
    state: StateRecord,
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
fn scan_chunks(chunks: &[(u64, PathBuf)]) -> RS<ScanReplay> {
    let mut state = StateRecord::default();
    let mut entries: BTreeMap<u64, LogEntry> = BTreeMap::new();
    let mut state_seen = false;
    let mut tail: Option<(u64, u64)> = None;
    let mut torn: Option<(usize, u64)> = None;
    let mut chunk_max: BTreeMap<u64, u64> = BTreeMap::new();
    let mut state_chunk_seq = 0u64;

    for (i, (seq, path)) in chunks.iter().enumerate() {
        let buf = fs_sync::read(path)?;
        let scanned = scan_valid_frame_prefix(&buf);
        let records = reassemble_records(&scanned.frames)?;
        chunk_max.insert(*seq, 0);
        for (_lsn, payload) in records {
            let record: StoreRecord = rmp_serde::from_slice(&payload)
                .map_err(|e| mudu_error!(ErrorCode::Decode, "decode raft store record error", e))?;
            match &record {
                StoreRecord::Entry(entry) => {
                    chunk_max.insert(*seq, entry.index);
                }
                StoreRecord::State(_) => {
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
fn repair_torn(chunks: &[(u64, PathBuf)], scan: &mut ScanReplay) -> RS<()> {
    if let Some((i, valid_len)) = scan.torn {
        let (seq, path) = &chunks[i];
        for (_, later) in chunks.iter().skip(i + 1) {
            fs_sync::remove_file(later)?;
        }
        if valid_len == 0 && i > 0 {
            fs_sync::remove_file(path)?;
            // tail stays at the previous chunk (set during the scan)
            scan.chunk_max.remove(seq);
            if scan.state_chunk_seq == *seq {
                scan.state_chunk_seq = 0;
            }
        } else {
            let file = open_chunk_file(path, false)?;
            file.set_len(valid_len)?;
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
    state: &StateRecord,
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
fn open_tail(dir: &Path, tail: Option<(u64, u64)>) -> RS<(u64, u64, Option<fs_sync::SFile>)> {
    let (tail_seq, tail_offset, tail_file) = match tail {
        Some((seq, offset)) => {
            let path = chunk_path(dir, seq);
            let mut file = open_chunk_file(&path, false)?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| mudu_error!(ErrorCode::Io, "seek raft chunk error", e))?;
            (seq, offset, Some(file))
        }
        None => (1, 0, None),
    };
    Ok((tail_seq, tail_offset, tail_file))
}

fn conf_from_value_version(value: &ConfValue, version: &ConfVersion) -> ConfNodeValue {
    ConfNodeValue::new(value.clone(), version.term, version.version, version.index)
}

fn apply_replay(
    record: StoreRecord,
    state: &mut StateRecord,
    entries: &mut BTreeMap<u64, LogEntry>,
    state_seen: &mut bool,
) {
    match record {
        StoreRecord::Entry(entry) => {
            entries.insert(entry.index, entry);
        }
        StoreRecord::TruncateRight { from_index } => {
            entries.retain(|idx, _| *idx < from_index);
        }
        StoreRecord::TruncateLeft { to_index } => {
            entries.retain(|idx, _| *idx > to_index);
        }
        StoreRecord::State(record) => {
            *state = *record;
            *state_seen = true;
        }
    }
}

fn chunk_path(dir: &Path, seq: u64) -> PathBuf {
    dir.join(format!("{seq:020}{CHUNK_SUFFIX}"))
}

fn list_chunks(dir: &Path) -> RS<Vec<(u64, PathBuf)>> {
    let mut chunks = Vec::new();
    for path in fs_sync::read_dir(dir)? {
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

fn open_chunk_file(path: &Path, create: bool) -> RS<fs_sync::SFile> {
    let mut opts = fs_sync::SOpenOptions::new();
    opts.read(true).write(true);
    if create {
        opts.create(true);
    }
    opts.open(path)
}

fn create_chunk_file(path: &Path) -> RS<fs_sync::SFile> {
    let mut opts = fs_sync::SOpenOptions::new();
    opts.read(true).write(true).create_new(true);
    match opts.open(path) {
        Ok(file) => Ok(file),
        Err(e) => {
            if e.ec() == ErrorCode::AlreadyExists {
                // a chunk with this sequence exists (e.g. left over from a
                // torn rotation); open and rewrite it from scratch
                let file = open_chunk_file(path, false)?;
                file.set_len(0)?;
                Ok(file)
            } else {
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests;
