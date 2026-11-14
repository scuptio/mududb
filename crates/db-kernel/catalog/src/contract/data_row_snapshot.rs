//! Owned row snapshots: no pointers or lock state are serialized.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Image {
    format: u32,
    tid: TupleID,
    tuple: Vec<(u64, u64, bool, Vec<u8>)>,
    delta: Vec<Delta>,
}
#[derive(Serialize, Deserialize)]
struct Delta {
    min: u64,
    max: u64,
    deleted: bool,
    edits: Vec<(u32, u32, Vec<u8>)>,
}

impl DataRow {
    /// Whether only the cache owns this row. A cache must serialize lookup
    /// and eviction before using this to evict; external clones pin the row.
    pub fn is_unshared(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }

    /// Approximate owned version storage, excluding allocator overhead.
    pub async fn resident_bytes(&self) -> usize {
        self.inner.lock().await.bytes
    }

    /// Accounts this row's live version bytes to one cache shard. Subsequent
    /// writes adjust the counter; the last row clone dropping releases it.
    pub async fn account_memory(&self, account: Arc<AtomicUsize>) -> RS<()> {
        let mut row = self.inner.lock().await;
        if let Some(existing) = &row.accounting {
            if !Arc::ptr_eq(existing, &account) {
                return Err(mudu_error!(
                    ErrorCode::Internal,
                    "row already belongs to another cache"
                ));
            }
        } else {
            account.fetch_add(row.bytes, Ordering::Relaxed);
            row.accounting = Some(account);
        }
        Ok(())
    }

    /// Copies a consistent complete version image under the async row lock,
    /// then encodes after releasing that lock.
    pub async fn export_snapshot(&self) -> RS<Vec<u8>> {
        let image = {
            let row = self.inner.lock().await;
            Image {
                format: 1,
                tid: row.tid,
                tuple: row
                    .tuple
                    .iter()
                    .map(|v| {
                        (
                            v.timestamp().c_min(),
                            v.timestamp().c_max(),
                            v.is_deleted(),
                            v.tuple().clone(),
                        )
                    })
                    .collect(),
                delta: row
                    .delta
                    .iter()
                    .map(|d| Delta {
                        min: d.timestamp().c_min(),
                        max: d.timestamp().c_max(),
                        deleted: d.is_deleted(),
                        edits: d
                            .update_delta()
                            .iter()
                            .map(|e| (e.offset(), e.to_up_size(), e.delta().clone()))
                            .collect(),
                    })
                    .collect(),
            }
        };
        serde_json::to_vec(&image)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode row snapshot", e))
    }

    /// Restores a complete row image from a validated cache record.
    pub fn restore_snapshot(bytes: &[u8]) -> RS<Self> {
        let image: Image = serde_json::from_slice(bytes)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode row snapshot", e))?;
        if image.format != 1
            || image.tuple.len() > UNCOMPRESSED_VERSION_COUNT
            || (image.tuple.is_empty() && !image.delta.is_empty())
            || (!image.tuple.is_empty() && image.delta.len() + 1 < image.tuple.len())
        {
            return Err(mudu_error!(ErrorCode::Decode, "invalid row snapshot shape"));
        }
        // Delta walks start at the oldest retained tuple, not the newest.
        // Shallow writes deliberately do not preserve payload transitions
        // between retained tuples; those tuples are read directly.
        let older = image
            .delta
            .len()
            .saturating_add(1)
            .saturating_sub(image.tuple.len());
        let mut size = image.tuple.first().map(|v| v.3.len() as u64).unwrap_or(0);
        for delta in image.delta[..older.min(image.delta.len())].iter().rev() {
            for (offset, removed, data) in &delta.edits {
                if u64::from(*offset) + u64::from(*removed) > size {
                    return Err(mudu_error!(
                        ErrorCode::Decode,
                        "row snapshot delta outside payload"
                    ));
                }
                size = size - u64::from(*removed) + data.len() as u64;
            }
        }
        let tuple: Vec<VersionTuple> = image
            .tuple
            .into_iter()
            .map(|(min, max, deleted, bytes)| {
                let ts = Timestamp::new(min, max);
                if deleted {
                    VersionTuple::new_delete(ts)
                } else {
                    VersionTuple::new(ts, bytes)
                }
            })
            .collect();
        let delta: Vec<VersionDelta> = image
            .delta
            .into_iter()
            .map(|d| {
                VersionDelta::new(
                    Timestamp::new(d.min, d.max),
                    d.deleted,
                    d.edits
                        .into_iter()
                        .map(|(offset, size, data)| UpdateDelta::new(offset, size, data))
                        .collect(),
                )
            })
            .collect();
        let mut row = DataRowInner::new(image.tid);
        for delta in delta {
            row.append_delta(delta);
        }
        for version in tuple {
            row.push_version(version);
        }
        Ok(Self {
            inner: Arc::new(AMutex::new(row)),
        })
    }
}
