//! Authoritative capture entrypoint; all appends/heal owners stay frozen.
use super::*;
use crate::log_stream::{GroupInstallJournal, GroupSnapshotBundle, StreamApplyScheduler};
use mudu_raft::stream::GroupIdobject;
use std::path::Path;

impl GenericReplicationGroup {
    /// Captures a complete local business bundle after a current-leader barrier.
    /// Appends and heal are excluded during capture. Registry/membership changes
    /// invalidate the attempt, and every business adapter must support snapshots.
    /// The returned bundle is not permission to compact or install Raft state.
    pub async fn capture_snapshot_bundle(
        &self,
        scheduler: &StreamApplyScheduler,
    ) -> RS<GroupSnapshotBundle> {
        let mut owners = Vec::with_capacity(self.streams.len());
        for slot in self.streams.values() {
            owners.push(slot.owner.lock().await);
        }
        self.ensure_running()?;
        let (term, state) = self.registry.leader_view().await?;
        for id in self.streams.keys() {
            self.validate_slot(*id, &state, false)?;
        }
        let (old, new) = self.raft.conf_async().await?;
        let bundle = scheduler
            .capture_snapshot_bundle(&state, old.clone(), new.clone())
            .await?;
        let (after_old, after_new) = self.raft.conf_async().await?;
        if old != after_old
            || new != after_new
            || old.node.conf_version.index != after_old.node.conf_version.index
            || new.node.conf_version.index != after_new.node.conf_version.index
            || self.registry.refresh().await?.frontier() != state.frontier()
        {
            return Err(invalid("group changed during snapshot capture; retry"));
        }
        self.ensure_running()?;
        self.ensure_leader(term)?;
        *self.snapshot.lock().await = Some(bundle.clone());
        Ok(bundle)
    }

    /// Pins the Raft snapshot frontier to the bundle's exact committed cut,
    /// making this durable bundle the authoritative snapshot at that frontier.
    /// The bundle must have been captured from this group at the current
    /// committed frontier: a foreign group or a drifted (stale/future) cut is
    /// rejected before the Raft snapshot moves, so a conflicting term never
    /// triggers the destructive snapshot-discard path. Advancing the frontier
    /// only authorizes later `force_compact_async` / per-stream `gc_prefix`;
    /// it neither compacts the log nor reclaims stream frames itself.
    pub async fn install_snapshot_frontier(&self, bundle: &GroupSnapshotBundle) -> RS<TermIndex> {
        self.ensure_running()?;
        let frontier = bundle.manifest.state.frontier();
        let state = self.registry.refresh().await?;
        if bundle.manifest.state.identity() != state.identity() {
            return Err(invalid("bundle belongs to a different group"));
        }
        if state.frontier() != frontier {
            return Err(invalid(
                "bundle cut differs from the current committed frontier",
            ));
        }
        self.raft.install_snapshot_async(frontier).await?;
        Ok(frontier)
    }

    /// Serves the most recently captured bundle to a RaftFollower that fell below
    /// the retained prefix. The bundle must already have been durably published
    /// by the caller; this only serializes the in-memory capture. A request for
    /// a foreign group is rejected.
    pub async fn serve_snapshot(&self, identity: GroupIdobject) -> RS<Vec<u8>> {
        self.ensure_running()?;
        let snapshot = self.snapshot.lock().await;
        let Some(bundle) = snapshot.as_ref() else {
            return Err(invalid("no group snapshot is available to serve"));
        };
        if bundle.manifest.state.identity() != identity {
            return Err(invalid("foreign group snapshot request"));
        }
        bundle.encode()
    }

    /// Fetches a complete group snapshot bundle from a peer, for a replica
    /// whose stream prefix fell below the retained frontier. The returned bytes
    /// are opaque and checksummed; decode and install them with
    /// [`GroupSnapshotBundle::decode`] and the offline install journal.
    pub async fn fetch_snapshot_from(&self, peer: u64) -> RS<Vec<u8>> {
        self.ensure_running()?;
        let session = self.transport.session(peer).await?;
        if !session.valid.is_valid() {
            return Err(invalid("stream session replaced"));
        }
        let identity = self.registry.refresh().await?.identity();
        self.transport
            .fetch_snapshot(peer, &session, identity)
            .await
    }

    /// Installs a group snapshot fetched from `peer` into this replica, for a
    /// replica whose stream prefix fell below the retained frontier. The caller
    /// must quiesce the apply driver before invoking. The bundle is fetched,
    /// validated and published, then the Raft snapshot frontier, the registry
    /// (in-memory and durable) and every stream log are reset to the bundle cut,
    /// under a durable journal so an interrupted install is redone on startup.
    pub async fn install_snapshot_from_peer(
        &self,
        peer: u64,
        bundle_dir: &Path,
        journal_dir: &Path,
    ) -> RS<()> {
        self.ensure_running()?;
        let identity = self.registry.refresh().await?.identity();
        let adapters = self.registry.adapters().clone();
        let bytes = self.fetch_snapshot_from(peer).await?;
        let bundle = GroupSnapshotBundle::decode(&bytes, identity, &adapters)?;
        bundle.publish(bundle_dir, &adapters).await?;
        let journal = GroupInstallJournal::begin(journal_dir, bundle_dir, &bundle).await?;
        self.raft
            .install_snapshot_async(bundle.manifest.state.frontier())
            .await?;
        self.registry
            .install_state(bundle.manifest.state.clone())
            .await?;
        let images = bundle.decode_logged_images(&adapters)?;
        for (id, image) in &images {
            let slot = self.slot(*id)?;
            slot.log.restore_snapshot(image.log()).await?;
        }
        journal.complete().await
    }

    /// Reclaims every stream record at or below the bundle's captured prefix,
    /// after the bundle has been durably published and pinned as the
    /// authoritative snapshot. A replica needing a reclaimed record recovers it
    /// via [`Self::install_snapshot_from_peer`]. This is one-way: the reclaimed
    /// history is no longer capturable, and a later snapshot must build on a
    /// compacted retry-index image (not yet implemented). Returns the reclaimed
    /// byte count per stream.
    pub async fn gc_below_bundle(&self, bundle: &GroupSnapshotBundle) -> RS<Vec<(StreamId, u64)>> {
        self.ensure_running()?;
        let state = self.registry.refresh().await?;
        if bundle.manifest.state.identity() != state.identity() {
            return Err(invalid("bundle belongs to a different group"));
        }
        if state.frontier() != bundle.manifest.state.frontier() {
            return Err(invalid("bundle cut differs from the committed frontier"));
        }
        let mut reclaimed = Vec::new();
        for (id, image) in &bundle.manifest.images {
            let Some(prefix) = &image.prefix else {
                continue;
            };
            let below = prefix
                .end_lsn
                .checked_add(1)
                .ok_or_else(|| invalid("stream LSN exhausted"))?;
            let slot = self.slot(*id)?;
            reclaimed.push((*id, slot.log.gc_prefix(below).await?));
        }
        Ok(reclaimed)
    }
}
