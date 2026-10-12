use super::*;
use std::io::Write;
use std::path::PathBuf;

impl WorkerLogLayout {
    /// Path of the persisted GC frontier marker of this log.
    pub fn gc_frontier_path(&self) -> PathBuf {
        self.log_dir
            .join(format!("{}{}", self.short_oid, GC_FRONTIER_SUFFIX))
    }

    /// Loads the persisted GC frontier, `None` when the log was never GC'd.
    pub fn load_gc_frontier(&self) -> RS<Option<GcFrontier>> {
        let path = self.gc_frontier_path();
        if !mudu_sys::fs::sync::sync_path_exists(&path) {
            return Ok(None);
        }
        let bytes = mudu_sys::fs::sync::read(&path)?;
        let frontier = rmp_serde::from_slice(&bytes).map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                format!("decode GC frontier file {} error", path.display()),
                e
            )
        })?;
        Ok(Some(frontier))
    }

    /// Persists the GC frontier (write + fsync). The marker is written
    /// BEFORE the chunks below it are deleted, so a crash mid-GC can only
    /// leave files the next GC round deletes again.
    pub fn store_gc_frontier(&self, frontier: GcFrontier) -> RS<()> {
        let path = self.gc_frontier_path();
        let bytes = rmp_serde::to_vec(&frontier)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode GC frontier error", e))?;
        let mut options = mudu_sys::fs::sync::SOpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut file = options.open(&path)?;
        file.write_all(&bytes)
            .map_err(|e| mudu_error!(ErrorCode::Io, "write GC frontier error", e))?;
        file.sync_data()?;
        Ok(())
    }

    pub(super) fn gc_frontier_or_zero(&self) -> GcFrontier {
        self.load_gc_frontier()
            .ok()
            .flatten()
            .unwrap_or(GcFrontier {
                lsn: 0,
                first_alive_seq: 0,
            })
    }

    /// Path of the persisted archive frontier marker of this log.
    pub fn archive_frontier_path(&self) -> PathBuf {
        self.log_dir
            .join(format!("{}{}", self.short_oid, ARCHIVE_FRONTIER_SUFFIX))
    }

    /// Loads the persisted archive frontier, `None` when archiving never ran.
    pub fn load_archive_frontier(&self) -> RS<Option<ArchiveFrontier>> {
        let path = self.archive_frontier_path();
        if !mudu_sys::fs::sync::sync_path_exists(&path) {
            return Ok(None);
        }
        let bytes = mudu_sys::fs::sync::read(&path)?;
        let frontier = rmp_serde::from_slice(&bytes).map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                format!("decode archive frontier file {} error", path.display()),
                e
            )
        })?;
        Ok(Some(frontier))
    }

    /// Persists the archive frontier (write + fsync). Written BEFORE the
    /// chunks below it are reported archived to the caller, so a crash mid
    /// archive only re-archives files the next round copies again.
    pub fn store_archive_frontier(&self, frontier: ArchiveFrontier) -> RS<()> {
        let path = self.archive_frontier_path();
        let bytes = rmp_serde::to_vec(&frontier)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode archive frontier error", e))?;
        let mut options = mudu_sys::fs::sync::SOpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut file = options.open(&path)?;
        file.write_all(&bytes)
            .map_err(|e| mudu_error!(ErrorCode::Io, "write archive frontier error", e))?;
        file.sync_data()?;
        Ok(())
    }

    pub(super) fn archive_frontier_or_zero(&self) -> ArchiveFrontier {
        self.load_archive_frontier()
            .ok()
            .flatten()
            .unwrap_or(ArchiveFrontier {
                lsn: 0,
                first_unarchived_seq: 0,
            })
    }
}
