//! The clean marker is the publication point for a rebuildable index checkpoint.
//! A candidate is invalidated before reuse; publication writes and syncs the body
//! before setting the clean marker, so interrupted work cannot appear complete.
use super::rows::checksum;
use index::paged::TreeCheckpoint;
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_sys::contract::async_file::AsyncFile;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(super) struct Manifest {
    pub format: u32,
    pub stamp: Vec<u64>,
    pub tree: [u64; 5],
    pub next_tuple: u64,
}
impl Manifest {
    pub fn new(stamp: Vec<u64>, tree: TreeCheckpoint, next_tuple: u64) -> Self {
        Self {
            format: 1,
            stamp,
            tree: [
                tree.root,
                tree.next_page,
                tree.count,
                tree.next_value,
                tree.page_size as u64,
            ],
            next_tuple,
        }
    }
    pub fn state(&self) -> TreeCheckpoint {
        TreeCheckpoint {
            root: self.tree[0],
            next_page: self.tree[1],
            count: self.tree[2],
            next_value: self.tree[3],
            page_size: self.tree[4] as usize,
        }
    }
}
fn number(bytes: &[u8], at: usize) -> u64 {
    let mut n = [0; 8];
    n.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(n)
}

/// Invalidate durably before any cache/authority mutation. Torn manifests
/// cannot become valid: close writes the body durably before the clean byte.
pub(super) async fn consume(file: &dyn AsyncFile) -> RS<Option<Manifest>> {
    let len = file.file_len().await?;
    let candidate = if len >= 17 {
        let header = file.read_exact_at(0, 17).await?;
        let size = number(&header, 1);
        if header[0] == 1 && size <= 65536 && size.checked_add(17).is_some_and(|n| n <= len) {
            let bytes = file.read_exact_at(17, size as usize).await?;
            if checksum(&bytes) == number(&header, 9) {
                serde_json::from_slice::<Manifest>(&bytes).ok()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    file.write_all_at(0, &[0]).await?;
    file.fsync().await?;
    Ok(candidate.filter(|m| m.format == 1 && m.next_tuple != 0))
}

pub(super) async fn publish(file: &dyn AsyncFile, manifest: &Manifest) -> RS<()> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode index checkpoint", e))?;
    file.write_all_at(0, &[0]).await?;
    file.fsync().await?;
    file.write_all_at(17, &bytes).await?;
    let mut header = (bytes.len() as u64).to_le_bytes().to_vec();
    header.extend_from_slice(&checksum(&bytes).to_le_bytes());
    file.write_all_at(1, &header).await?;
    file.fsync().await?;
    file.write_all_at(0, &[1]).await?;
    file.fsync().await
}
