use super::*;
use std::collections::BTreeSet;

impl GroupSnapshotManifest {
    /// Validates identity, registry history, memberships and exact image coverage.
    pub fn validate(&self, identity: GroupIdobject, adapters: &AdapterCapabilities) -> RS<()> {
        if self.version != 1 || self.state.identity() != identity {
            return Err(invalid("snapshot format or group identity mismatch"));
        }
        self.state.validate(adapters)?;
        membership(self)?;
        let live: BTreeSet<_> = self
            .state
            .streams()
            .iter()
            .filter(|(_, s)| s.lifecycle != StreamLifecycle::Tombstoned)
            .map(|(id, _)| *id)
            .collect();
        if live != self.images.keys().copied().collect() {
            return Err(invalid(
                "snapshot does not contain exactly the live streams",
            ));
        }
        let mut total = 0u64;
        for (id, image) in &self.images {
            let descriptor = &self.state.streams()[id].descriptor;
            if image.prefix.as_ref() != self.state.committed().get(id)
                || image.file != image_name(*id, descriptor.stream_generation)
                || image.len > MAX_IMAGE_BYTES
            {
                return Err(invalid(
                    "snapshot image identity, prefix, path or size mismatch",
                ));
            }
            total = total
                .checked_add(image.len)
                .ok_or_else(|| invalid("snapshot size overflow"))?;
        }
        if total > MAX_BUNDLE_BYTES {
            return Err(invalid("group snapshot exceeds total budget"));
        }
        Ok(())
    }
}

pub(super) fn membership(manifest: &GroupSnapshotManifest) -> RS<()> {
    let cut = manifest.state.frontier();
    for conf in [&manifest.committed_conf, &manifest.incoming_conf] {
        let version = &conf.node.conf_version;
        let logs: BTreeSet<_> = conf.node.nid_log.iter().copied().collect();
        let voters: BTreeSet<_> = conf.node.nid_vote.iter().copied().collect();
        let peers: BTreeSet<_> = conf.value.node_peer.iter().map(|p| p.node_id).collect();
        let voting: BTreeSet<_> = conf
            .value
            .node_peer
            .iter()
            .filter(|p| p.can_vote)
            .map(|p| p.node_id)
            .collect();
        if version.version == 0
            || version.index > cut.index
            || version.term > cut.term
            || voters.is_empty()
            || logs.len() != conf.node.nid_log.len()
            || voters.len() != conf.node.nid_vote.len()
            || peers.len() != conf.value.node_peer.len()
            || logs != peers
            || voters != voting
            || !voters.is_subset(&logs)
        {
            return Err(invalid(
                "snapshot membership is invalid or newer than its Raft cut",
            ));
        }
    }
    let old = &manifest.committed_conf.node.conf_version;
    let new = &manifest.incoming_conf.node.conf_version;
    if new < old
        || (new == old
            && (new.index != old.index || manifest.committed_conf != manifest.incoming_conf))
    {
        return Err(invalid("snapshot joint membership versions conflict"));
    }
    Ok(())
}
