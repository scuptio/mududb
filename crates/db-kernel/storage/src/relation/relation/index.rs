//! Feature selection is centralized here; all relation callers await the index.
#[cfg(not(feature = "paged-index"))]
use ::index::btree::btree_index::BTreeIndex;
use ::index::index_key::{compare_context::CompareContext, key_tuple::KeyTuple};
use catalog::contract::data_row::DataRow;
use mudu::common::result::RS;
use mudu_sys::contract::async_fs::AsyncFs;
use std::{ops::Bound, path::Path, sync::Arc};

pub(super) struct RelationIndex {
    #[cfg(feature = "paged-index")]
    inner: super::paged::Handle,
    #[cfg(not(feature = "paged-index"))]
    inner: mudu_sys::sync::async_::AMutex<BTreeIndex<DataRow>>,
}
impl RelationIndex {
    pub async fn open(
        context: CompareContext,
        fs: &Arc<dyn AsyncFs>,
        base: &Path,
        stamp: &[u64],
    ) -> RS<(Self, Option<u64>)> {
        #[cfg(feature = "paged-index")]
        {
            let (fs, base, stamp) = (fs.clone(), base.to_owned(), stamp.to_vec());
            let (inner, next) = tokio::task::spawn_local(async move {
                let (relation, next) =
                    super::paged::PagedRelation::open(context, &fs, &base, &stamp).await?;
                Ok::<_, mudu::error::MuduError>((super::paged::Handle::new(relation)?, next))
            })
            .await
            .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Storage, e.to_string()))??;
            Ok((Self { inner }, next))
        }
        #[cfg(not(feature = "paged-index"))]
        {
            let _ = (fs, base, stamp);
            Ok((
                Self {
                    inner: mudu_sys::sync::async_::AMutex::new(BTreeIndex::new(context)),
                },
                None,
            ))
        }
    }
    pub async fn get(&self, key: &KeyTuple) -> RS<Option<DataRow>> {
        #[cfg(feature = "paged-index")]
        {
            let key = key.clone();
            self.inner
                .call(move |tree| Box::pin(async move { tree.get(&key).await }))
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            self.inner.lock().await.get(key)
        }
    }
    pub async fn insert(&self, key: KeyTuple, value: DataRow) -> RS<Option<DataRow>> {
        #[cfg(feature = "paged-index")]
        {
            self.inner
                .call(move |tree| Box::pin(async move { tree.insert(key, value).await }))
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            self.inner.lock().await.insert(key, value)
        }
    }

    /// Batch point lookups through one mailbox round trip (paged) or one lock
    /// acquisition (baseline). Input order is preserved.
    pub async fn get_batch(&self, keys: &[KeyTuple]) -> RS<Vec<Option<DataRow>>> {
        #[cfg(feature = "paged-index")]
        {
            let keys = keys.to_vec();
            self.inner
                .call(move |tree| Box::pin(async move { tree.get_batch(&keys).await }))
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            let mut found = Vec::with_capacity(keys.len());
            let inner = self.inner.lock().await;
            for key in keys {
                found.push(inner.get(key)?);
            }
            Ok(found)
        }
    }

    /// Inserts rows whose absence the caller already proved under its stripe
    /// guards; see `PagedRelation::insert_reserved_batch`.
    pub async fn insert_reserved_batch(&self, pairs: Vec<(KeyTuple, DataRow)>) -> RS<()> {
        #[cfg(feature = "paged-index")]
        {
            self.inner
                .call(move |tree| Box::pin(async move { tree.insert_reserved_batch(pairs).await }))
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            let inner = self.inner.lock().await;
            for (key, row) in pairs {
                if inner.insert(key, row)?.is_some() {
                    return Err(mudu::mudu_error!(
                        mudu::error::ErrorCode::Internal,
                        "reserved row insert displaced an existing mapping"
                    ));
                }
            }
            Ok(())
        }
    }
    pub async fn range(
        &self,
        bounds: (Bound<&KeyTuple>, Bound<&KeyTuple>),
    ) -> RS<Vec<(KeyTuple, DataRow)>> {
        #[cfg(feature = "paged-index")]
        {
            let bounds = (bounds.0.cloned(), bounds.1.cloned());
            self.inner
                .call(move |tree| {
                    Box::pin(
                        async move { tree.range((bounds.0.as_ref(), bounds.1.as_ref())).await },
                    )
                })
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            self.inner.lock().await.range(bounds)
        }
    }
    pub async fn flush(&self) -> RS<()> {
        #[cfg(feature = "paged-index")]
        {
            self.inner
                .call(|tree| Box::pin(async move { tree.flush().await }))
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            Ok(())
        }
    }

    pub async fn before_write(&self) -> RS<()> {
        #[cfg(feature = "paged-index")]
        self.inner
            .call(|tree| Box::pin(async move { tree.before_write().await }))
            .await?;
        Ok(())
    }

    pub async fn maintain(&self) -> RS<::index::paged::MaintenanceStats> {
        #[cfg(feature = "paged-index")]
        {
            self.inner
                .call(|tree| Box::pin(async move { tree.maintain().await }))
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            Ok(::index::paged::MaintenanceStats::default())
        }
    }

    pub async fn checkpoint(&self, stamp: Vec<u64>, next_tuple: u64, close: bool) -> RS<()> {
        #[cfg(feature = "paged-index")]
        {
            self.inner
                .call(move |tree| {
                    Box::pin(async move { tree.checkpoint(stamp, next_tuple, close).await })
                })
                .await
        }
        #[cfg(not(feature = "paged-index"))]
        {
            let _ = (stamp, next_tuple, close);
            Ok(())
        }
    }
}
