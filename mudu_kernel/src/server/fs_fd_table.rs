//! Session-scoped file descriptor tables for the fs object syscalls.
//!
//! Each session owns one [`FdTable`]; fds are `u32` handles that follow the
//! POSIX model: the minimum free value is allocated and closed values are
//! reused. All state lives behind atomics and `Arc` so [`super::fs_service`]
//! methods only need `&self`.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_file::AsyncFile;
use scc::HashMap as SccHashMap;

/// One open fs object entry file bound to a session fd.
pub(crate) struct FdEntry {
    /// Object id this fd is bound to.
    pub oid: OID,
    /// Filesystem id of the fs type the object belongs to.
    pub fs_id: u64,
    /// Kind code of the object (`FsTypeKind::as_u32`).
    pub kind: u32,
    /// Partition hosting the object's `_fs_object` row.
    pub partition_id: OID,
    /// Generation the fd is anchored to.
    pub generation: u64,
    /// Normalized object-relative entry path (empty for FILE objects).
    pub entry_rel: String,
    /// Open content file.
    pub file: Arc<dyn AsyncFile>,
    /// Read/write cursor.
    pub cursor: AtomicU64,
    /// Current content length in bytes (grows on writes).
    pub length: AtomicU64,
    /// Whether the fd was opened with read access.
    pub read: bool,
    /// Whether the fd was opened with write access.
    pub write: bool,
}

/// Parameters for binding a new fd entry.
pub(crate) struct FdEntryParams {
    /// Object id this fd is bound to.
    pub oid: OID,
    /// Filesystem id of the fs type the object belongs to.
    pub fs_id: u64,
    /// Kind code of the object (`FsTypeKind::as_u32`).
    pub kind: u32,
    /// Partition hosting the object's `_fs_object` row.
    pub partition_id: OID,
    /// Generation the fd is anchored to.
    pub generation: u64,
    /// Normalized object-relative entry path (empty for FILE objects).
    pub entry_rel: String,
    /// Open content file.
    pub file: Arc<dyn AsyncFile>,
    /// Initial content length in bytes (0 for write fds).
    pub length: u64,
    /// Whether the fd was opened with read access.
    pub read: bool,
    /// Whether the fd was opened with write access.
    pub write: bool,
}

impl FdEntry {
    /// Create a new fd entry with cursor 0 and the given initial length.
    pub(crate) fn new(params: FdEntryParams) -> Self {
        Self {
            oid: params.oid,
            fs_id: params.fs_id,
            kind: params.kind,
            partition_id: params.partition_id,
            generation: params.generation,
            entry_rel: params.entry_rel,
            file: params.file,
            cursor: AtomicU64::new(0),
            length: AtomicU64::new(params.length),
            read: params.read,
            write: params.write,
        }
    }

    /// Return the current cursor.
    pub(crate) fn cursor(&self) -> u64 {
        self.cursor.load(Ordering::Relaxed)
    }

    /// Set the cursor.
    pub(crate) fn set_cursor(&self, cursor: u64) {
        self.cursor.store(cursor, Ordering::Relaxed);
    }

    /// Return the current content length.
    pub(crate) fn length(&self) -> u64 {
        self.length.load(Ordering::Relaxed)
    }

    /// Grow the content length to at least `length`.
    pub(crate) fn grow_length(&self, length: u64) {
        self.length.fetch_max(length, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for FdEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FdEntry")
            .field("oid", &self.oid)
            .field("fs_id", &self.fs_id)
            .field("kind", &self.kind)
            .field("partition_id", &self.partition_id)
            .field("generation", &self.generation)
            .field("entry_rel", &self.entry_rel)
            .field("cursor", &self.cursor())
            .field("length", &self.length())
            .field("read", &self.read)
            .field("write", &self.write)
            .finish_non_exhaustive()
    }
}

/// Per-session fd table.
struct FdTable {
    entries: SccHashMap<u32, Arc<FdEntry>>,
    next_hint: AtomicU32,
}

impl FdTable {
    fn new() -> Self {
        Self {
            entries: SccHashMap::new(),
            next_hint: AtomicU32::new(0),
        }
    }

    /// Allocate the minimum free fd and bind `entry` to it.
    fn insert(&self, entry: Arc<FdEntry>) -> RS<u32> {
        let mut candidate = self.next_hint.load(Ordering::Relaxed);
        loop {
            match self.entries.entry_sync(candidate) {
                scc::hash_map::Entry::Occupied(_) => {
                    candidate = candidate.checked_add(1).ok_or_else(|| {
                        mudu_error!(ErrorCode::InvalidState, "session fd space exhausted")
                    })?;
                }
                scc::hash_map::Entry::Vacant(slot) => {
                    slot.insert_entry(entry);
                    self.next_hint.store(candidate + 1, Ordering::Relaxed);
                    return Ok(candidate);
                }
            }
        }
    }

    /// Return the entry bound to `fd`, if any.
    fn get(&self, fd: u32) -> Option<Arc<FdEntry>> {
        self.entries.get_sync(&fd).map(|entry| entry.get().clone())
    }

    /// Remove and return the entry bound to `fd`, if any.
    fn remove(&self, fd: u32) -> Option<Arc<FdEntry>> {
        let removed = self.entries.remove_sync(&fd).map(|(_fd, entry)| entry);
        if removed.is_some() {
            // Keep the hint at or below the lowest freed fd so allocation
            // still finds the minimum free value.
            self.next_hint.fetch_min(fd, Ordering::Relaxed);
        }
        removed
    }
}

/// Registry of the fd tables of all sessions of one worker.
pub(crate) struct FsFdTables {
    tables: SccHashMap<OID, FdTable>,
}

impl FsFdTables {
    /// Create an empty registry.
    pub(crate) fn new() -> Self {
        Self {
            tables: SccHashMap::new(),
        }
    }

    /// Allocate a session-local fd and bind `entry` to it.
    pub(crate) fn insert(&self, session_id: OID, entry: Arc<FdEntry>) -> RS<u32> {
        loop {
            match self.tables.entry_sync(session_id) {
                scc::hash_map::Entry::Occupied(occupied) => return occupied.get().insert(entry),
                scc::hash_map::Entry::Vacant(slot) => {
                    slot.insert_entry(FdTable::new());
                }
            }
        }
    }

    /// Return the entry bound to `fd` in the session's table.
    pub(crate) fn get(&self, session_id: OID, fd: u32) -> RS<Arc<FdEntry>> {
        self.tables
            .get_sync(&session_id)
            .and_then(|table| table.get().get(fd))
            .ok_or_else(|| bad_fd(fd))
    }

    /// Remove and return the entry bound to `fd` in the session's table.
    pub(crate) fn remove(&self, session_id: OID, fd: u32) -> RS<Arc<FdEntry>> {
        self.tables
            .get_sync(&session_id)
            .and_then(|table| table.get().remove(fd))
            .ok_or_else(|| bad_fd(fd))
    }

    /// Drop the session's whole fd table, if it exists.
    pub(crate) fn remove_session(&self, session_id: OID) {
        let _ = self.tables.remove_sync(&session_id);
    }

    /// Return the number of open fds of a session (test support).
    #[cfg(test)]
    pub(crate) fn session_fd_count(&self, session_id: OID) -> usize {
        self.tables
            .get_sync(&session_id)
            .map(|table| table.get().entries.len())
            .unwrap_or(0)
    }
}

fn bad_fd(fd: u32) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::BadFileDescriptor,
        format!("fs fd {} is not open", fd)
    )
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use async_trait::async_trait;

    struct NoopFile;

    #[async_trait]
    impl AsyncFile for NoopFile {
        async fn read_exact_at(&self, _offset: u64, _len: usize) -> RS<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn write_all_at(&self, _offset: u64, _payload: &[u8]) -> RS<()> {
            Ok(())
        }

        async fn fsync(&self) -> RS<()> {
            Ok(())
        }

        async fn file_len(&self) -> RS<u64> {
            Ok(0)
        }
    }

    fn test_entry() -> Arc<FdEntry> {
        Arc::new(FdEntry::new(FdEntryParams {
            oid: 1,
            fs_id: 2,
            kind: 1,
            partition_id: 0,
            generation: 0,
            entry_rel: String::new(),
            file: Arc::new(NoopFile),
            length: 0,
            read: true,
            write: false,
        }))
    }

    #[test]
    fn fds_allocate_minimum_free_value_and_reuse_after_close() {
        let tables = FsFdTables::new();
        let session = 42;
        let fd0 = tables.insert(session, test_entry()).unwrap();
        let fd1 = tables.insert(session, test_entry()).unwrap();
        let fd2 = tables.insert(session, test_entry()).unwrap();
        assert_eq!((fd0, fd1, fd2), (0, 1, 2));

        tables.remove(session, fd1).unwrap();
        // The freed value is reused before higher values are handed out.
        let fd = tables.insert(session, test_entry()).unwrap();
        assert_eq!(fd, fd1);
    }

    #[test]
    fn fd_tables_are_isolated_per_session() {
        let tables = FsFdTables::new();
        let fd_a = tables.insert(1, test_entry()).unwrap();
        let fd_b = tables.insert(2, test_entry()).unwrap();
        assert_eq!(fd_a, fd_b);
        assert!(tables.get(1, fd_a).is_ok());
        assert!(tables.get(2, fd_a).is_ok());
        tables.remove(2, fd_b).unwrap();
        assert!(tables.get(2, fd_b).is_err());
        assert!(tables.get(1, fd_a).is_ok());
    }

    #[test]
    fn unknown_fd_reports_bad_file_descriptor() {
        let tables = FsFdTables::new();
        let err = tables.get(7, 0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = tables.remove(7, 3).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    }

    #[test]
    fn remove_session_reclaims_the_whole_table() {
        let tables = FsFdTables::new();
        let session = 9;
        for _ in 0..3 {
            tables.insert(session, test_entry()).unwrap();
        }
        assert_eq!(tables.session_fd_count(session), 3);
        tables.remove_session(session);
        assert_eq!(tables.session_fd_count(session), 0);
        // Removing again is a no-op.
        tables.remove_session(session);
        assert_eq!(tables.session_fd_count(session), 0);
    }

    #[test]
    fn entry_state_uses_atomics() {
        let entry = test_entry();
        assert_eq!(entry.cursor(), 0);
        assert_eq!(entry.length(), 0);
        entry.set_cursor(11);
        entry.grow_length(5);
        entry.grow_length(3);
        assert_eq!(entry.cursor(), 11);
        assert_eq!(entry.length(), 5);
    }
}
