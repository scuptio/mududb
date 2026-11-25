//! Retain submitted writes when their caller is cancelled. A subsequent read,
//! write or flush first drives the retained future to completion, preventing a
//! late kernel completion from overwriting a newer retry at the same offset.
use async_trait::async_trait;
use futures::{
    future::{BoxFuture, Shared},
    FutureExt,
};
use mudu::common::result::RS;
use mudu_sys::{contract::async_file::AsyncFile, sync::async_::AMutex};
use std::sync::Arc;

type Pending = Shared<BoxFuture<'static, RS<()>>>;

/// Serializes writes to one async file and retains in-flight writes across
/// caller cancellation. Reads/flushes drain pending writes before proceeding.
pub struct OrderedFile {
    inner: Arc<dyn AsyncFile>,
    // The wrapper owns a future clone, so cancellation cannot discard submitted bytes.
    pending: AMutex<Option<Pending>>,
}
impl OrderedFile {
    /// Wraps a file. All overlapping writes must use the same wrapper.
    pub fn new(inner: Arc<dyn AsyncFile>) -> Self {
        Self {
            inner,
            pending: AMutex::new(None),
        }
    }
    /// Completes a previously cancelled write, if any.
    pub async fn drain(&self) -> RS<()> {
        let mut guard = self.pending.lock().await;
        if let Some(write) = guard.as_ref() {
            let result = write.clone().await;
            *guard = None;
            result?;
        }
        Ok(())
    }
}
#[async_trait]
impl AsyncFile for OrderedFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.drain().await?;
        self.inner.read_exact_at(offset, len).await
    }
    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        let mut guard = self.pending.lock().await;
        if let Some(write) = guard.as_ref() {
            let result = write.clone().await;
            *guard = None;
            result?;
        }
        let file = self.inner.clone();
        let bytes = payload.to_vec();
        let write = async move { file.write_all_at(offset, &bytes).await }
            .boxed()
            .shared();
        *guard = Some(write.clone());
        let result = write.await;
        *guard = None;
        result
    }
    async fn fsync(&self) -> RS<()> {
        self.drain().await?;
        self.inner.fsync().await
    }
    async fn file_len(&self) -> RS<u64> {
        self.drain().await?;
        self.inner.file_len().await
    }
    async fn close(&self) -> RS<()> {
        self.drain().await?;
        self.inner.close().await
    }
}
