//! One atomically published directory image; corruption never selects old state.
use super::*;
use std::collections::BTreeSet;
use std::path::Path;
const FILE: &str = "placement_overlay.mp";
const MAX_BYTES: u64 = 64 * 1024 * 1024;
type Entries = HashMap<(String, String), (PlacementOverride, u64)>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct OverlayCursor {
    pub(super) version: u64,
    pub(super) payload: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
struct Image {
    format: u32,
    cluster_id: u64,
    cursor: OverlayCursor,
    entries: Vec<(String, String, PlacementOverride, u64)>,
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    crc: u64,
    payload: Vec<u8>,
}

pub(super) fn load_overlay(dir: &Path, cluster_id: u64) -> RS<(Entries, Option<OverlayCursor>)> {
    // No legacy fallback: an old slot may be newer than a surviving new image.
    for old in ["placement_overlay_a.mp", "placement_overlay_b.mp"] {
        if exists(&dir.join(old))? {
            return Err(invalid(
                "legacy placement overlay requires explicit migration",
            ));
        }
    }
    let path = dir.join(FILE);
    // Listing propagates permission/I/O errors instead of treating them as absence.
    if !exists(dir)? {
        return Ok((HashMap::new(), None));
    }
    if !mudu_sys::fs::sync::read_dir_entries(dir)?
        .iter()
        .any(|entry| entry.path() == path)
    {
        return Ok((HashMap::new(), None));
    }
    if mudu_sys::fs::sync::sync_metadata(&path)?.len() > MAX_BYTES {
        return Err(invalid("placement image exceeds size budget"));
    }
    let bytes = mudu_sys::fs::sync::sync_read_all(&path)?;
    let envelope: Envelope = rmp_serde::from_slice(&bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode placement envelope", e))?;
    if mudu::common::crc::crc64(&envelope.payload) != envelope.crc {
        return Err(invalid("placement checksum mismatch"));
    }
    let image: Image = rmp_serde::from_slice(&envelope.payload)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode placement image", e))?;
    if image.format != 1 || image.cluster_id != cluster_id {
        return Err(invalid("placement image format or cluster mismatch"));
    }
    let op: MetaOp = rmp_serde::from_slice(&image.cursor.payload)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode placement cursor", e))?;
    if !matches!(
        op,
        MetaOp::BeginMigration { .. }
            | MetaOp::CompleteMigration { .. }
            | MetaOp::AbortMigration { .. }
            | MetaOp::SplitPartition { .. }
    ) {
        return Err(invalid(
            "placement cursor contains a non-placement operation",
        ));
    }
    let mut keys = BTreeSet::new();
    let mut entries = HashMap::new();
    for (rule, partition, value, version) in image.entries {
        if version > image.cursor.version || !keys.insert((rule.clone(), partition.clone())) {
            return Err(invalid(
                "placement image contains duplicate or future entries",
            ));
        }
        entries.insert((rule, partition), (value, version));
    }
    Ok((entries, Some(image.cursor)))
}

pub(super) async fn save_overlay(
    dir: &Path,
    cluster_id: u64,
    entries: &Entries,
    cursor: &OverlayCursor,
) -> RS<()> {
    let mut entries: Vec<_> = entries
        .iter()
        .map(|((rule, partition), (value, version))| {
            (rule.clone(), partition.clone(), value.clone(), *version)
        })
        .collect();
    entries.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let payload = rmp_serde::to_vec(&Image {
        format: 1,
        cluster_id,
        cursor: cursor.clone(),
        entries,
    })
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode placement image", e))?;
    let bytes = rmp_serde::to_vec(&Envelope {
        crc: mudu::common::crc::crc64(&payload),
        payload,
    })
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode placement envelope", e))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("placement image exceeds size budget"));
    }
    let fs = mudu_sys::fs::tokio_fs();
    repl::durable_io::ensure_dir(fs.as_ref(), dir).await?;
    repl::durable_io::publish(fs.as_ref(), &dir.join(FILE), &bytes).await
}
fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}

// Resolve absence through successful parent listings, never through a boolean
// metadata probe that treats permission or I/O errors as "not found".
fn exists(path: &Path) -> RS<bool> {
    let Some(name) = path.file_name() else {
        mudu_sys::fs::sync::sync_metadata(if path.as_os_str().is_empty() {
            Path::new(".")
        } else {
            path
        })?;
        return Ok(true);
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !exists(parent)? {
        return Ok(false);
    }
    Ok(mudu_sys::fs::sync::read_dir_entries(parent)?
        .iter()
        .any(|entry| entry.path().file_name() == Some(name)))
}

#[cfg(test)]
mod tests;
