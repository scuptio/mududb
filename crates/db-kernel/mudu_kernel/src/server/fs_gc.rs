//! Fs object generation garbage collection.
//!
//! [`FsGc`] reclaims on-disk generations of fs objects, working against the
//! flat layout and the prefix-match rule documented in
//! [`super::fs_service`]. It runs in two modes:
//!
//! - [`FsGc::recover_scan`] runs once at startup after WAL replay with a
//!   see-everything snapshot. A generation is kept iff its object row exists,
//!   points at that generation, and is SEALED; everything else (aborted
//!   writes, replaced generations, rows that never committed) is removed.
//! - [`FsGc::gc_once`] runs periodically against a horizon snapshot: an
//!   object whose row is visible at the horizon keeps the row's current
//!   generation and loses every other generation; an object with no visible
//!   row (deleted, or never committed) loses all of its generations.
//!
//! Both modes also remove the whole storage root of an fs id that is no
//! longer registered in the fs type catalog.
//!
//! The periodic driver is transport-specific: the tokio backend runs
//! [`FsGc::gc_loop`], which sleeps between rounds and exits on its stop
//! channel. The io_uring worker loop cannot park a task on the tokio timer
//! (its service loop drives the ring synchronously), so it re-spawns
//! one-round [`FsGc::gc_round`] tasks from the service loop instead; see
//! `linux/worker_ring_loop.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::tokio;
use tracing::{error, trace};

use crate::contract::meta_mgr::MetaMgr;
use crate::meta::fs_object::FS_OBJECT_STATE_SEALED;
use crate::meta::fs_type_catalog::fs_storage_base;
use crate::server::fs_service::FsObjectStore;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::server::x_contract::WorkerXContract;

/// Interval between two fs GC rounds.
pub(crate) const FS_GC_INTERVAL: Duration = Duration::from_secs(60);

/// Fs object generation garbage collector: startup recovery scan plus
/// periodic horizon-based reclamation; see the module docs.
pub(crate) struct FsGc {
    data_dir: String,
    fs: Arc<dyn AsyncFs>,
    meta_mgr: Arc<dyn MetaMgr>,
    object_store: Arc<dyn FsObjectStore>,
    snapshot_source: Arc<WorkerXContract>,
}

/// Root entries of one fs id storage root, grouped by object id and then by
/// generation.
type FsRootGenerations = BTreeMap<OID, BTreeMap<u64, Vec<PathBuf>>>;

impl FsGc {
    pub(crate) fn new(
        data_dir: String,
        fs: Arc<dyn AsyncFs>,
        meta_mgr: Arc<dyn MetaMgr>,
        object_store: Arc<dyn FsObjectStore>,
        snapshot_source: Arc<WorkerXContract>,
    ) -> Self {
        Self {
            data_dir,
            fs,
            meta_mgr,
            object_store,
            snapshot_source,
        }
    }

    /// Startup recovery scan; see the module docs. Runs after WAL replay so
    /// every committed `_fs_object` row is visible.
    pub(crate) async fn recover_scan(&self) -> RS<()> {
        let see_everything = WorkerSnapshot::new(u64::MAX, Vec::new());
        for (_fs_id, objects) in self.scan_fs_roots().await? {
            for (oid, generations) in objects {
                let row = self
                    .object_store
                    .read_fs_object_committed(oid, &see_everything)
                    .await?;
                for (generation, entries) in generations {
                    let keep = match &row {
                        Some((_partition_id, row)) => {
                            row.generation == generation && row.state == FS_OBJECT_STATE_SEALED
                        }
                        None => false,
                    };
                    if !keep {
                        self.remove_root_entries(entries).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// One periodic GC round against `horizon`; see the module docs.
    pub(crate) async fn gc_once(&self, horizon: &WorkerSnapshot) -> RS<()> {
        for (_fs_id, objects) in self.scan_fs_roots().await? {
            for (oid, generations) in objects {
                let current = self
                    .object_store
                    .read_fs_object_committed(oid, horizon)
                    .await?
                    .map(|(_partition_id, row)| row.generation);
                for (generation, entries) in generations {
                    if current != Some(generation) {
                        self.remove_root_entries(entries).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Run one periodic round: compute the horizon and reclaim with it.
    pub(crate) async fn gc_round(&self) -> RS<()> {
        let horizon = self.gc_horizon()?;
        self.gc_once(&horizon).await
    }

    /// Periodic GC loop for executors with a working timer (the tokio
    /// backend). Sleeps `interval` between rounds but wakes immediately when
    /// `stop_rx` fires; a failing round is logged and retried at the next
    /// tick.
    pub(crate) async fn gc_loop(&self, interval: Duration, mut stop_rx: StopRx) -> RS<()> {
        loop {
            tokio::select! {
                _ = mudu_sys::task::async_::sleep(interval) => {}
                changed = stop_rx.changed() => {
                    if !changed || stop_rx.is_stopped() {
                        break;
                    }
                }
            }
            if stop_rx.is_stopped() {
                break;
            }
            if let Err(err) = self.gc_round().await {
                error!("fs gc round failed, {}", err);
            }
        }
        Ok(())
    }

    /// Compute the GC horizon: the oldest snapshot that still observes every
    /// generation a running transaction may reference — the oldest running
    /// xid, or the newest allocated xid when no transaction is running.
    fn gc_horizon(&self) -> RS<WorkerSnapshot> {
        let xid = match self.snapshot_source.oldest_running_xid()? {
            Some(xid) => xid,
            None => self.snapshot_source.latest_xid(),
        };
        Ok(WorkerSnapshot::new(xid, Vec::new()))
    }

    /// Enumerate every fs storage root, removing roots whose fs id is no
    /// longer registered in the catalog, and group the remaining root
    /// entries by object id and generation. Names that do not parse as
    /// `{oidhex}.{generation}[.{entry}]` are left untouched.
    async fn scan_fs_roots(&self) -> RS<Vec<(u64, FsRootGenerations)>> {
        let fs_base = fs_storage_base(&self.data_dir);
        if !self.fs.path_exists(&fs_base).await? {
            return Ok(Vec::new());
        }
        let mut roots = Vec::new();
        for entry in self.fs.read_dir(&fs_base).await? {
            let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Ok(fs_id) = name.parse::<u64>() else {
                continue;
            };
            if self.meta_mgr.get_fs_type_by_id(fs_id).await?.is_none() {
                // The fs type was dropped: its whole storage root is garbage.
                self.remove_root_entry(&entry).await?;
                continue;
            }
            let mut objects: FsRootGenerations = BTreeMap::new();
            for path in self.fs.read_dir(&entry).await? {
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some((oid, generation)) = parse_generation_name(name) else {
                    continue;
                };
                objects
                    .entry(oid)
                    .or_default()
                    .entry(generation)
                    .or_default()
                    .push(path);
            }
            roots.push((fs_id, objects));
        }
        Ok(roots)
    }

    /// Remove every fs root entry of one reclaimed generation.
    async fn remove_root_entries(&self, entries: Vec<PathBuf>) -> RS<()> {
        for entry in entries {
            self.remove_root_entry(&entry).await?;
        }
        Ok(())
    }

    /// Remove one fs root entry: directories recursively, files directly.
    async fn remove_root_entry(&self, path: &Path) -> RS<()> {
        trace!(path = %path.display(), "fs gc removing entry");
        if self.fs.read_dir(path).await.is_ok() {
            self.fs.remove_dir_all(path).await
        } else {
            self.fs.remove_file_if_exists(path).await
        }
    }
}

/// Parse an fs root entry name as `{oidhex}.{generation}[.{entry}]`.
///
/// `oidhex` is exactly 32 lowercase hex characters and `generation` a
/// decimal u64; anything else is a name the flat layout never produces and
/// parses to `None`.
fn parse_generation_name(name: &str) -> Option<(OID, u64)> {
    let (oid_hex, rest) = name.split_once('.')?;
    if oid_hex.len() != 32
        || !oid_hex
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return None;
    }
    let oid = OID::from_str_radix(oid_hex, 16).ok()?;
    let generation_text = rest.split('.').next().filter(|text| !text.is_empty())?;
    if !generation_text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let generation = generation_text.parse::<u64>().ok()?;
    Some((oid, generation))
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

    #[test]
    fn parse_generation_name_accepts_flat_layout_names() {
        let oid = 0xF500_0000_0000_0001u128;
        assert_eq!(
            parse_generation_name(&format!("{oid:032x}.12")),
            Some((oid, 12))
        );
        assert_eq!(
            parse_generation_name(&format!("{oid:032x}.12.a/b")),
            Some((oid, 12))
        );
        assert_eq!(
            parse_generation_name(&format!("{oid:032x}.0")),
            Some((oid, 0))
        );
    }

    #[test]
    fn parse_generation_name_rejects_foreign_names() {
        let oid = 0xF500_0000_0000_0001u128;
        // No dot at all.
        assert_eq!(parse_generation_name("README"), None);
        // Short oid component.
        assert_eq!(parse_generation_name("abcd.1"), None);
        // Uppercase hex is not produced by the layout.
        assert_eq!(parse_generation_name(&format!("{oid:032X}.1")), None);
        // Non-decimal or empty generation component.
        assert_eq!(parse_generation_name(&format!("{oid:032x}.x")), None);
        assert_eq!(parse_generation_name(&format!("{oid:032x}..a")), None);
        assert_eq!(parse_generation_name(&format!("{oid:032x}.12a")), None);
        // Generation overflows u64.
        assert_eq!(
            parse_generation_name(&format!("{oid:032x}.99999999999999999999999999")),
            None
        );
    }
}
