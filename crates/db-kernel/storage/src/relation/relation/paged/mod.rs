//! Disk-backed row-ID tree and rebuildable async row-version cache.
mod handle;
mod manifest;
pub use handle::Handle;
mod rows;
use catalog::contract::data_row::DataRow;
use index::{
    index_key::{compare_context::CompareContext, key_tuple::KeyTuple},
    paged::{BufferOptions, FilePageStore, OrderedFile, PagedBTree},
};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_sys::contract::{async_file::AsyncFile, async_fs::AsyncFs, file_options::FileOptions};
use std::{ops::Bound, path::Path, sync::Arc};

pub struct PagedRelation {
    tree: PagedBTree<u128>,
    rows: rows::Rows,
    manifest: Arc<dyn AsyncFile>,
    // Serializes durable invalidation before the first mutation after a checkpoint.
    dirty: mudu_sys::sync::async_::AMutex<bool>,
}

impl PagedRelation {
    pub async fn open(
        context: CompareContext,
        fs: &Arc<dyn AsyncFs>,
        base: &Path,
        stamp: &[u64],
    ) -> RS<(Self, Option<u64>)> {
        let name = base.to_string_lossy();
        let options = FileOptions::read_write_create();
        let manifest = fs
            .open(Path::new(&format!("{name}.index-manifest")), options)
            .await?;
        let manifest: Arc<dyn AsyncFile> = Arc::new(OrderedFile::new(manifest));
        let mut candidate = manifest::consume(manifest.as_ref()).await?;
        for suffix in ["index-pages", "index-rows", "index-row-directory"] {
            if !fs
                .path_exists(Path::new(&format!("{name}.{suffix}")))
                .await?
            {
                candidate = None;
            }
        }
        candidate = candidate.filter(|m| m.stamp == stamp);
        let options = FileOptions {
            truncate: candidate.is_none(),
            ..options
        };
        let page_file = fs
            .open(Path::new(&format!("{name}.index-pages")), options)
            .await?;
        let row_data = fs
            .open(Path::new(&format!("{name}.index-rows")), options)
            .await?;
        let row_directory = fs
            .open(Path::new(&format!("{name}.index-row-directory")), options)
            .await?;
        let row_data: Arc<dyn AsyncFile> = Arc::new(OrderedFile::new(row_data));
        let row_directory: Arc<dyn AsyncFile> = Arc::new(OrderedFile::new(row_directory));
        let config = BufferOptions::default();
        let store = Arc::new(FilePageStore::new(page_file, config.page_size)?);
        let mut restored = None;
        let tree = if let Some(candidate) = candidate {
            match PagedBTree::restore_row_ids(
                context.clone(),
                config,
                store.clone(),
                &candidate.state(),
            )
            .await
            {
                Ok(tree) => {
                    restored = Some(candidate.next_tuple);
                    tree
                }
                Err(_) => {
                    // No valid checkpoint is reachable now. Reclaim cache
                    // files before rebuilding from the authoritative streams.
                    for suffix in ["index-pages", "index-rows", "index-row-directory"] {
                        fs.open(
                            Path::new(&format!("{name}.{suffix}")),
                            FileOptions {
                                truncate: true,
                                ..options
                            },
                        )
                        .await?
                        .close()
                        .await?;
                    }
                    PagedBTree::with_row_ids(context, config, store)?
                }
            }
        } else {
            PagedBTree::with_row_ids(context, config, store)?
        };
        Ok((
            Self {
                tree,
                rows: rows::Rows::new(row_data, row_directory).await?,
                manifest,
                dirty: mudu_sys::sync::async_::AMutex::new(true),
            },
            restored,
        ))
    }
    pub async fn get(&self, key: &KeyTuple) -> RS<Option<DataRow>> {
        match self.tree.get(key).await? {
            Some(id) => Ok(Some(self.rows.get(id).await?)),
            None => Ok(None),
        }
    }
    pub async fn insert(&self, key: KeyTuple, row: DataRow) -> RS<Option<DataRow>> {
        let old = self.get(&key).await?;
        let id = row
            .tuple_id()
            .await?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing row ID"))?;
        if let Some(old) = &old {
            if old.tuple_id().await? != Some(id) {
                return Err(mudu_error!(ErrorCode::Decode, "row identity changed"));
            }
        }
        self.rows.put(row).await?;
        self.tree.insert(key, id).await?;
        Ok(old)
    }

    /// Batch point lookups: one tree lease for all keys, then the row cache
    /// for the hits. Input order is preserved.
    pub async fn get_batch(&self, keys: &[KeyTuple]) -> RS<Vec<Option<DataRow>>> {
        let ids = self.tree.get_batch(keys).await?;
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            found.push(match id {
                Some(id) => Some(self.rows.get(id).await?),
                None => None,
            });
        }
        Ok(found)
    }

    /// Inserts rows whose absence the caller already proved under its stripe
    /// guards (see `resolve_row_tuples`): skips the per-insert identity
    /// re-read that `insert` performs. Every returned old value must be
    /// empty; anything else means the absence proof was unsound.
    pub async fn insert_reserved_batch(&self, pairs: Vec<(KeyTuple, DataRow)>) -> RS<()> {
        let mut id_pairs = Vec::with_capacity(pairs.len());
        for (key, row) in pairs {
            let id = row
                .tuple_id()
                .await?
                .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing row ID"))?;
            self.rows.put(row).await?;
            id_pairs.push((key, id));
        }
        let olds = self.tree.insert_batch(&id_pairs).await?;
        if olds.iter().any(|old| old.is_some()) {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "reserved row insert displaced an existing mapping"
            ));
        }
        Ok(())
    }
    pub async fn range(
        &self,
        bounds: (Bound<&KeyTuple>, Bound<&KeyTuple>),
    ) -> RS<Vec<(KeyTuple, DataRow)>> {
        let ids = self.tree.range(bounds).await?;
        let mut rows = Vec::with_capacity(ids.len());
        for (key, id) in ids {
            rows.push((key, self.rows.get(id).await?));
        }
        Ok(rows)
    }
    pub async fn flush(&self) -> RS<()> {
        self.tree.flush_pages().await
    }

    // The relation lifecycle gate orders this against checkpoint publication.
    // Retain false across cancellation: the next writer must retry invalidation.
    pub async fn before_write(&self) -> RS<()> {
        let mut dirty = self.dirty.lock().await;
        if !*dirty {
            self.manifest.write_all_at(0, &[0]).await?;
            self.manifest.fsync().await?;
            *dirty = true;
        }
        Ok(())
    }

    pub async fn maintain(&self) -> RS<::index::paged::MaintenanceStats> {
        self.before_write().await?;
        self.tree.maintain().await
    }

    pub async fn checkpoint(&self, stamp: Vec<u64>, next_tuple: u64, close: bool) -> RS<()> {
        self.before_write().await?;
        let tree = if close {
            self.tree.close_checkpoint().await?
        } else {
            self.tree.checkpoint().await?
        };
        self.rows.flush().await?;
        let mut dirty = self.dirty.lock().await;
        // Publication may be cancelled after submitting the clean-byte write.
        // Every subsequent writer must invalidate even when publish returns Err.
        *dirty = false;
        manifest::publish(
            self.manifest.as_ref(),
            &manifest::Manifest::new(stamp, tree, next_tuple),
        )
        .await
    }
}

#[cfg(test)]
mod checkpoint_tests;
