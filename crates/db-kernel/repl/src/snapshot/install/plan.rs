//! Frozen ownership-scoped removal plan for exact snapshot installation.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct FilePlan {
    pub partitions: BTreeSet<OID>,
    removed: BTreeSet<String>,
}

impl FilePlan {
    pub fn capture(
        data_dir: &str,
        manifest: &ReplSnapshotManifest,
        partitions: &[OID],
    ) -> RS<Self> {
        let mut scope: BTreeSet<_> = partitions.iter().copied().collect();
        if manifest.worker_index == 0 {
            scope.insert(0);
        }
        if scope.is_empty() || (manifest.worker_index != 0 && scope.contains(&0)) {
            return Err(invalid("invalid snapshot partition ownership"));
        }
        let mut plan = Self {
            partitions: scope,
            removed: BTreeSet::new(),
        };
        plan.validate_manifest(manifest)?;
        let included: BTreeSet<_> = manifest
            .files
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        for path in plan.live_files(data_dir, manifest.worker_index)? {
            if path.ends_with(".dat") && !included.contains(path.as_str()) {
                plan.removed.insert(path);
            }
        }
        Ok(plan)
    }

    fn owns(&self, path: &str, worker_index: u32) -> bool {
        let Some((dir, name)) = path.rsplit_once('/') else {
            return false;
        };
        if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
            return false;
        }
        if !(name.ends_with(".dat") || name.ends_with(".index-manifest")) {
            return false;
        }
        match dir {
            "relation" => name
                .split_once('.')
                .and_then(|(p, _)| p.parse::<OID>().ok())
                .is_some_and(|p| self.partitions.contains(&p)),
            "meta/relation" => worker_index == 0,
            _ => false,
        }
    }

    pub fn validate_manifest(&self, manifest: &ReplSnapshotManifest) -> RS<()> {
        for file in &manifest.files {
            if file.path.ends_with(".dat") && !self.owns(&file.path, manifest.worker_index) {
                return Err(invalid(
                    "snapshot relation is outside the target worker's partitions",
                ));
            }
        }
        for path in &self.removed {
            if !self.owns(path, manifest.worker_index)
                || !path.ends_with(".dat")
                || manifest.files.iter().any(|file| file.path == *path)
            {
                return Err(invalid("invalid snapshot removal plan"));
            }
        }
        Ok(())
    }

    fn live_files(&self, data_dir: &str, worker_index: u32) -> RS<Vec<String>> {
        let mut paths = Vec::new();
        for dir in ["relation", "meta/relation"] {
            if dir.starts_with("meta") && worker_index != 0 {
                continue;
            }
            let root = Path::new(data_dir).join(dir);
            if !mudu_sys::fs::sync::sync_path_exists(&root) {
                continue;
            }
            for path in mudu_sys::fs::sync::read_dir(root)? {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let relative = format!("{dir}/{name}");
                    if self.owns(&relative, worker_index) {
                        paths.push(relative);
                    }
                }
            }
        }
        Ok(paths)
    }

    pub fn validate_live(&self, data_dir: &str, manifest: &ReplSnapshotManifest) -> RS<()> {
        self.validate_manifest(manifest)?;
        for path in self.live_files(data_dir, manifest.worker_index)? {
            if path.ends_with(".dat")
                && !self.removed.contains(&path)
                && !manifest.files.iter().any(|file| file.path == path)
            {
                return Err(invalid(
                    "relation files changed after snapshot install intent",
                ));
            }
        }
        Ok(())
    }

    pub async fn apply(&self, data_dir: &str, manifest: &ReplSnapshotManifest) -> RS<()> {
        let source = snapshot_version_dir(data_dir, manifest.worker_index, manifest.committed_lsn);
        if super::super::manifest::read_sealed(&source)?.as_ref() != Some(manifest) {
            return Err(invalid("snapshot source changed before file installation"));
        }
        self.validate_live(data_dir, manifest)?;
        let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
        // Invalidate clean-index markers durably BEFORE replacing any data.
        // Index pages/rows are disposable caches rebuilt by relation open.
        for path in self.live_files(data_dir, manifest.worker_index)? {
            if path.ends_with(".index-manifest") {
                fs.remove_file_if_exists(&Path::new(data_dir).join(path))
                    .await?;
            }
        }
        self.sync_dirs(data_dir, fs.as_ref()).await?;
        install_snapshot_relation_files(data_dir, &source, manifest).await?;
        for path in &self.removed {
            fs.remove_file_if_exists(&Path::new(data_dir).join(path))
                .await?;
        }
        self.sync_dirs(data_dir, fs.as_ref()).await
    }

    async fn sync_dirs(&self, data_dir: &str, fs: &dyn AsyncFs) -> RS<()> {
        for dir in ["relation", "meta/relation"] {
            let path = Path::new(data_dir).join(dir);
            if fs.path_exists(&path).await? {
                fs.sync_dir(&path).await?;
            }
        }
        Ok(())
    }
}

pub(super) fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}
