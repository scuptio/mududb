//! Validation and durable publication of sealed worker snapshots.
use super::*;
use std::collections::BTreeSet;

/// Checks the supported format and canonical file set before any file I/O.
pub fn validate_snapshot_manifest(manifest: &ReplSnapshotManifest) -> RS<()> {
    if manifest.version != 2 || manifest.committed_lsn == u64::MAX {
        return Err(invalid("unsupported snapshot version or exhausted LSN"));
    }
    let mut paths = BTreeSet::new();
    for entry in &manifest.files {
        validate_path(&entry.path)?;
        if !paths.insert(entry.path.as_str()) {
            return Err(invalid("duplicate snapshot file"));
        }
        if entry.path.starts_with("meta/") && manifest.worker_index != 0 {
            return Err(invalid("only worker zero may snapshot shared catalogs"));
        }
    }
    if !paths.contains(KV_FILE_NAME) || !paths.contains(recovery::RECOVERY_FILE) {
        return Err(invalid("snapshot is missing its KV or recovery image"));
    }
    Ok(())
}

pub(super) fn validate_path(path: &str) -> RS<()> {
    let parts: Vec<_> = path.split('/').collect();
    if path.contains('\\')
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return Err(invalid("noncanonical snapshot file path"));
    }
    let valid = match parts.as_slice() {
        ["kv.mp"] | ["recovery.mp"] => true,
        ["relation", name] | ["meta", "relation", name] => name.ends_with(".dat"),
        _ => false,
    };
    if !valid {
        return Err(invalid("unknown snapshot file path"));
    }
    Ok(())
}

pub(super) fn read_sealed(dir: &Path) -> RS<Option<ReplSnapshotManifest>> {
    let path = dir.join(MANIFEST_FILE_NAME);
    if !mudu_sys::fs::sync::sync_path_exists(&path) {
        return Ok(None);
    }
    let manifest = rmp_serde::from_slice(&mudu_sys::fs::sync::read(path)?).map_err(|error| {
        mudu_error!(ErrorCode::CorruptedData, "invalid snapshot manifest", error)
    })?;
    verify_snapshot_files(dir, &manifest)?;
    Ok(Some(manifest))
}

pub(super) fn reusable_snapshot(
    dir: &Path,
    worker: u32,
    lsn: u64,
    digest: SegHash,
) -> RS<Option<ReplSnapshotManifest>> {
    let Some(manifest) = read_sealed(dir)? else {
        return Ok(None);
    };
    if manifest.worker_index != worker
        || manifest.committed_lsn != lsn
        || (digest != crate::seg_hash::ZERO_SEG_HASH && manifest.seg_hash != digest)
    {
        return Err(invalid("existing snapshot belongs to a different cut"));
    }
    Ok(Some(manifest))
}

/// Verifies all files before publishing the manifest, then fsyncs the
/// directory entries before allowing callers to reclaim older snapshots.
pub async fn seal_snapshot(dir: &Path, manifest: &ReplSnapshotManifest) -> RS<()> {
    if let Some(existing) = read_sealed(dir)? {
        return if existing == *manifest {
            Ok(())
        } else {
            Err(invalid("refusing to replace a sealed snapshot manifest"))
        };
    }
    verify_snapshot_files(dir, manifest)?;
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    for entry in &manifest.files {
        let file = fs
            .open(&dir.join(&entry.path), FileOptions::read_only())
            .await?;
        file.fsync().await?;
        file.close().await?;
    }
    for child in ["relation", "meta/relation", "meta"] {
        let path = dir.join(child);
        if fs.path_exists(&path).await? {
            fs.sync_dir(&path).await?;
        }
    }
    fs.sync_dir(dir).await?;
    let bytes = rmp_serde::to_vec(manifest)
        .map_err(|error| mudu_error!(ErrorCode::Encode, "encode snapshot manifest", error))?;
    crate::durable_io::publish(fs.as_ref(), &dir.join(MANIFEST_FILE_NAME), &bytes).await?;
    if let Some(parent) = dir.parent() {
        fs.sync_dir(parent).await?;
    }
    Ok(())
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}

pub(super) fn validate_page_length(actual: u64, requested: u64) -> RS<()> {
    if actual > requested || (actual == 0 && requested > 0) {
        return Err(invalid(
            "snapshot page is empty or exceeds requested length",
        ));
    }
    Ok(())
}
