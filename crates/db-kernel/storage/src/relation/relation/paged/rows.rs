//! Row clones are pins: lookup and eviction are serialized per cache shard.
use catalog::contract::data_row::DataRow;
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_sys::{contract::async_file::AsyncFile, sync::async_::AMutex};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};

const SHARDS: usize = 16;
const BUDGET: usize = 4 * 1024 * 1024;
struct Shard {
    rows: HashMap<u128, DataRow>,
    clock: VecDeque<u128>,
    usage: Arc<AtomicUsize>,
    // Last successfully persisted content hash; unchanged eviction needs no rewrite.
    persisted: HashMap<u128, u64>,
}
pub(super) struct Rows {
    shards: Vec<AMutex<Shard>>,
    data: Arc<dyn AsyncFile>,
    directory: Arc<dyn AsyncFile>,
    next: AtomicU64,
    budget: usize,
}

pub(super) fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn error(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::Decode, message)
}
fn offset(id: u128) -> RS<u64> {
    u64::try_from(id)
        .ok()
        .and_then(|id| id.checked_mul(24))
        .ok_or_else(|| error("row directory offset overflow"))
}
fn number(bytes: &[u8], at: usize) -> u64 {
    let mut n = [0; 8];
    n.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(n)
}

impl Rows {
    pub async fn new(data: Arc<dyn AsyncFile>, directory: Arc<dyn AsyncFile>) -> RS<Self> {
        let next = data.file_len().await?;
        Ok(Self {
            data,
            directory,
            next: AtomicU64::new(next),
            budget: BUDGET,
            shards: (0..SHARDS)
                .map(|_| {
                    AMutex::new(Shard {
                        rows: HashMap::new(),
                        persisted: HashMap::new(),
                        clock: VecDeque::new(),
                        usage: Arc::new(AtomicUsize::new(0)),
                    })
                })
                .collect(),
        })
    }
    pub async fn get(&self, id: u128) -> RS<DataRow> {
        let mut shard = self.shards[id as usize % SHARDS].lock().await;
        if let Some(row) = shard.rows.get(&id).cloned() {
            self.trim(&mut shard).await?;
            return Ok(row);
        }
        let (row, hash) = self.load(id).await?;
        shard.persisted.insert(id, hash);
        // The returned clone pins the inserted row while trim runs.
        row.account_memory(shard.usage.clone()).await?;
        shard.rows.insert(id, row.clone());
        shard.clock.push_back(id);
        self.trim(&mut shard).await?;
        Ok(row)
    }
    pub async fn put(&self, row: DataRow) -> RS<()> {
        let id = row
            .tuple_id()
            .await?
            .ok_or_else(|| error("missing row ID"))?;
        offset(id)?;
        let mut shard = self.shards[id as usize % SHARDS].lock().await;
        if !shard.rows.contains_key(&id) {
            shard.clock.push_back(id);
        }
        row.account_memory(shard.usage.clone()).await?;
        shard.rows.insert(id, row);
        self.trim(&mut shard).await
    }
    async fn trim(&self, shard: &mut Shard) -> RS<()> {
        for _ in 0..shard.clock.len() {
            if shard.usage.load(Ordering::Relaxed) <= self.budget {
                break;
            }
            let Some(id) = shard.clock.pop_front() else {
                break;
            };
            let Some(row) = shard.rows.get(&id) else {
                continue;
            };
            // Keep the queue entry through cancellation or an I/O error.
            shard.clock.push_back(id);
            if !row.is_unshared() {
                continue;
            }
            // Forget the old digest before issuing writes: cancellation may
            // publish a new directory entry without completing this future.
            let previous = shard.persisted.remove(&id);
            self.save(id, row, previous).await?;
            shard.rows.remove(&id);
            shard.clock.pop_back();
        }
        Ok(())
    }
    async fn save(&self, id: u128, row: &DataRow, previous: Option<u64>) -> RS<u64> {
        let bytes = row.export_snapshot().await?;
        let hash = checksum(&bytes);
        // Reads may evict after an online checkpoint. Never rewrite an unchanged
        // durable directory entry to point at an unflushed new extent.
        if previous == Some(hash) {
            return Ok(hash);
        }
        let len = bytes.len() as u64;
        let at = self
            .next
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(len))
            .map_err(|_| error("row cache file exhausted"))?;
        self.data.write_all_at(at, &bytes).await?;
        let mut entry = at.to_le_bytes().to_vec();
        entry.extend_from_slice(&len.to_le_bytes());
        // Persist the checksum already computed for this immutable snapshot.
        entry.extend_from_slice(&hash.to_le_bytes());
        self.directory.write_all_at(offset(id)?, &entry).await?;
        Ok(hash)
    }
    async fn load(&self, id: u128) -> RS<(DataRow, u64)> {
        let entry = self.directory.read_exact_at(offset(id)?, 24).await?;
        if entry.len() != 24 {
            return Err(error("truncated row directory"));
        }
        let (at, len) = (number(&entry, 0), number(&entry, 8));
        let end = at
            .checked_add(len)
            .ok_or_else(|| error("invalid row extent"))?;
        if len == 0 || end > self.data.file_len().await? || len > usize::MAX as u64 {
            return Err(error("invalid row extent"));
        }
        let bytes = self.data.read_exact_at(at, len as usize).await?;
        if checksum(&bytes) != number(&entry, 16) {
            return Err(error("row cache checksum mismatch"));
        }
        let row = DataRow::restore_snapshot(&bytes)?;
        if row.tuple_id().await? != Some(id) {
            return Err(error("row cache identity mismatch"));
        }
        Ok((row, number(&entry, 16)))
    }
    pub async fn flush(&self) -> RS<()> {
        for shard in &self.shards {
            let mut shard = shard.lock().await;
            let Shard {
                rows, persisted, ..
            } = &mut *shard;
            for (id, row) in rows.iter() {
                let previous = persisted.remove(id);
                let hash = self.save(*id, row, previous).await?;
                persisted.insert(*id, hash);
            }
        }
        self.data.fsync().await?;
        self.directory.fsync().await
    }
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
