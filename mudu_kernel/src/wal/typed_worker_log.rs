use crate::wal::log_frame::decode_entries_with_pending;
use crate::wal::lsn::LSN;
use crate::wal::worker_log::{
    decode_frames_allow_trailing, AsyncWorkerLogRecoverySource, WorkerLogBackend,
    WorkerLogRecoverySource,
};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::scoped_task_trace;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::marker::PhantomData;

#[async_trait]
pub trait WorkerLogRecoveryHandler<L>: Send + Sync + 'static
where
    L: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    async fn handle_entry(&self, entry: L, start_lsn: LSN) -> RS<()>;

    fn finish(&self) -> RS<()> {
        Ok(())
    }
}

#[async_trait]
pub trait AsyncWorkerLogRecoveryHandler<L>: Send + Sync + 'static
where
    L: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    async fn handle_entry(&self, entry: L, start_lsn: LSN) -> RS<()>;

    async fn finish(&self) -> RS<()> {
        Ok(())
    }
}

pub struct TypedWorkerLog<L, B, H>
where
    L: Serialize + DeserializeOwned + Send + Sync + 'static,
    B: WorkerLogBackend,
    H: WorkerLogRecoveryHandler<L>,
{
    backend: B,
    handler: H,
    _marker: PhantomData<fn() -> L>,
}

impl<L, B, H> TypedWorkerLog<L, B, H>
where
    L: Serialize + DeserializeOwned + Send + Sync + 'static,
    B: WorkerLogBackend,
    H: WorkerLogRecoveryHandler<L>,
{
    pub fn new(backend: B, handler: H) -> Self {
        Self {
            backend,
            handler,
            _marker: PhantomData,
        }
    }

    /// Returns a reference to the wrapped backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub async fn append(&self, entry: &L) -> RS<()> {
        let frames = self.backend.serialize_entry(entry)?;
        self.backend.append_frames_async(frames).await
    }

    pub async fn append_callback<R, F>(&self, entry: L, callback: F) -> RS<()>
    where
        F: Fn(&L) -> RS<R>,
    {
        self.append(&entry).await?;
        self.flush_async().await?;
        let _ = callback(&entry)?;
        Ok(())
    }

    pub async fn append_async_callback<R, F, Fut>(&self, entry: L, callback: F) -> RS<()>
    where
        F: Fn(&L) -> Fut,
        Fut: Future<Output = RS<R>>,
    {
        self.append(&entry).await?;
        self.flush_async().await?;
        let _ = callback(&entry).await?;
        Ok(())
    }

    pub fn flush(&self) -> RS<()> {
        self.backend.flush()
    }

    pub async fn flush_async(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        self.backend.flush_async().await
    }

    pub async fn recover<S>(&self, source: &mut S) -> RS<()>
    where
        S: WorkerLogRecoverySource,
    {
        let chunk_paths = source.chunk_paths_sorted().await?;
        let mut pending_frames = Vec::new();
        let mut pending_start_lsn = None;
        for path in chunk_paths {
            let bytes = source.read_chunk(path.as_path()).await?;
            if bytes.is_empty() {
                continue;
            }
            let frames = decode_frames_allow_trailing(&bytes)?;
            let entries = decode_entries_with_pending::<L>(
                &frames,
                &mut pending_frames,
                &mut pending_start_lsn,
            )?;
            for (start_lsn, entry) in entries {
                self.handler.handle_entry(entry, start_lsn).await?;
            }
        }

        if !pending_frames.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Decode,
                "trailing partial log frames"
            ));
        }

        self.handler.finish()
    }

    pub async fn recover_async<S>(&self, source: &mut S) -> RS<()>
    where
        S: AsyncWorkerLogRecoverySource,
    {
        let chunk_paths = source.chunk_paths_sorted().await?;
        let mut pending_frames = Vec::new();
        let mut pending_start_lsn = None;
        for path in chunk_paths {
            let bytes = source.read_chunk(path.as_path()).await?;
            if bytes.is_empty() {
                continue;
            }
            let frames = decode_frames_allow_trailing(&bytes)?;
            let entries = decode_entries_with_pending::<L>(
                &frames,
                &mut pending_frames,
                &mut pending_start_lsn,
            )?;
            for (start_lsn, entry) in entries {
                self.handler.handle_entry(entry, start_lsn).await?;
            }
        }

        if !pending_frames.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Decode,
                "trailing partial log frames"
            ));
        }

        self.handler.finish()
    }

    pub async fn recover_async_with_handler<S, AH>(&self, source: &mut S, handler: &AH) -> RS<()>
    where
        S: AsyncWorkerLogRecoverySource,
        AH: AsyncWorkerLogRecoveryHandler<L>,
    {
        scoped_task_trace!();
        let chunk_paths = source.chunk_paths_sorted().await?;
        let mut pending_frames = Vec::new();
        let mut pending_start_lsn = None;
        for path in chunk_paths {
            let bytes = source.read_chunk(path.as_path()).await?;
            if bytes.is_empty() {
                continue;
            }
            let frames = decode_frames_allow_trailing(&bytes)?;
            let entries = decode_entries_with_pending::<L>(
                &frames,
                &mut pending_frames,
                &mut pending_start_lsn,
            )?;
            for (start_lsn, entry) in entries {
                handler.handle_entry(entry, start_lsn).await?;
            }
        }

        if !pending_frames.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Decode,
                "trailing partial log frames"
            ));
        }

        handler.finish().await
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::wal::worker_log::{
        AsyncWorkerLogRecoverySource, ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogLayout,
        WorkerLogRecoverySource,
    };
    use async_trait::async_trait;
    use mudu_sys::env_var::temp_dir;
    use mudu_sys::sync::SMutex;
    use mudu_utils::oid::gen_oid;
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestEntry {
        id: u64,
        payload: Vec<u8>,
    }

    #[derive(Default)]
    struct CollectingHandler {
        entries: SMutex<Vec<(LSN, TestEntry)>>,
    }

    #[async_trait]
    impl WorkerLogRecoveryHandler<TestEntry> for Arc<CollectingHandler> {
        async fn handle_entry(&self, entry: TestEntry, start_lsn: LSN) -> RS<()> {
            self.entries.lock().unwrap().push((start_lsn, entry));
            Ok(())
        }
    }

    #[async_trait]
    impl AsyncWorkerLogRecoveryHandler<TestEntry> for Arc<CollectingHandler> {
        async fn handle_entry(&self, entry: TestEntry, start_lsn: LSN) -> RS<()> {
            self.entries.lock().unwrap().push((start_lsn, entry));
            Ok(())
        }
    }

    struct FileRecoverySource {
        paths: Vec<PathBuf>,
    }

    struct NoopHandler;

    #[async_trait]
    impl WorkerLogRecoveryHandler<TestEntry> for NoopHandler {
        async fn handle_entry(&self, _entry: TestEntry, _start_lsn: LSN) -> RS<()> {
            Ok(())
        }
    }
    #[async_trait]
    impl WorkerLogRecoverySource for FileRecoverySource {
        async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
            Ok(self.paths.clone())
        }

        async fn read_chunk(&self, path: &Path) -> RS<Vec<u8>> {
            mudu_sys::fs::sync::read(path)
        }
    }

    #[async_trait]
    impl AsyncWorkerLogRecoverySource for FileRecoverySource {
        async fn chunk_paths_sorted(&mut self) -> RS<Vec<PathBuf>> {
            Ok(self.paths.clone())
        }

        async fn read_chunk(&mut self, path: &Path) -> RS<Vec<u8>> {
            mudu_sys::fs::async_::read_all(path).await
        }
    }

    #[test]
    fn typed_worker_log_appends_and_recovers_generic_entries() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = temp_dir().join(format!("typed_worker_log_{}", gen_oid()));
            let raw =
                ChunkedWorkerLogBackend::new(WorkerLogLayout::new(dir, gen_oid(), 256).unwrap())
                    .await
                    .unwrap();
            let handler = Arc::new(CollectingHandler::default());
            let log = TypedWorkerLog::new(raw.clone(), handler.clone());

            let first = TestEntry {
                id: 1,
                payload: vec![1; 32],
            };
            let second = TestEntry {
                id: 2,
                payload: vec![2; 512],
            };

            log.append(&first).await.unwrap();
            log.append(&second).await.unwrap();
            raw.flush_async().await.unwrap();
            let mut source = FileRecoverySource {
                paths: raw.chunk_paths_sorted().await.unwrap(),
            };
            log.recover(&mut source).await.unwrap();

            let recovered = handler.entries.lock().unwrap().clone();
            assert_eq!(recovered, vec![(LSN::new(0), first), (LSN::new(1), second)]);
        })
        .unwrap()
    }

    #[test]
    fn typed_worker_log_appends_and_recovers_generic_entries_async() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = temp_dir().join(format!("typed_worker_log_async_{}", gen_oid()));
            let raw =
                ChunkedWorkerLogBackend::new(WorkerLogLayout::new(dir, gen_oid(), 256).unwrap())
                    .await
                    .unwrap();
            let handler = Arc::new(CollectingHandler::default());
            let log = TypedWorkerLog::new(raw.clone(), handler.clone());

            let first = TestEntry {
                id: 1,
                payload: vec![1; 32],
            };
            let second = TestEntry {
                id: 2,
                payload: vec![2; 512],
            };

            log.append(&first).await.unwrap();
            log.append(&second).await.unwrap();
            raw.flush_async().await.unwrap();
            let mut source = FileRecoverySource {
                paths: raw.chunk_paths_sorted().await.unwrap(),
            };
            log.recover_async(&mut source).await.unwrap();

            let recovered = handler.entries.lock().unwrap().clone();
            assert_eq!(recovered, vec![(LSN::new(0), first), (LSN::new(1), second)]);
        })
        .unwrap()
    }

    #[test]
    fn typed_worker_log_recovers_with_async_handler() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = temp_dir().join(format!("typed_worker_log_async_handler_{}", gen_oid()));
            let raw =
                ChunkedWorkerLogBackend::new(WorkerLogLayout::new(dir, gen_oid(), 256).unwrap())
                    .await
                    .unwrap();
            let writer = TypedWorkerLog::new(raw.clone(), NoopHandler);
            let handler = Arc::new(CollectingHandler::default());

            let first = TestEntry {
                id: 11,
                payload: vec![3; 64],
            };
            let second = TestEntry {
                id: 12,
                payload: vec![4; 128],
            };

            writer.append(&first).await.unwrap();
            writer.append(&second).await.unwrap();
            raw.flush_async().await.unwrap();

            let mut source = FileRecoverySource {
                paths: raw.chunk_paths_sorted().await.unwrap(),
            };
            writer
                .recover_async_with_handler(&mut source, &handler)
                .await
                .unwrap();

            let recovered = handler.entries.lock().unwrap().clone();
            assert_eq!(recovered, vec![(LSN::new(0), first), (LSN::new(1), second)]);
        })
        .unwrap()
    }

    #[test]
    fn typed_worker_log_append_callback_runs_after_log_is_persisted() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = temp_dir().join(format!("typed_worker_log_append_callback_{}", gen_oid()));
            let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
            let path = layout.chunk_path(0);
            let backend = ChunkedWorkerLogBackend::new(layout).await.unwrap();
            let log = TypedWorkerLog::new(backend, NoopHandler);
            let entry = TestEntry {
                id: 7,
                payload: vec![7; 32],
            };

            let expected = entry.clone();
            let callback_path = path.clone();
            log.append_callback(entry, move |written| {
                assert_eq!(written, &expected);
                let bytes = mudu_sys::fs::sync::read(&callback_path)?;
                assert!(!bytes.is_empty());
                Ok(())
            })
            .await
            .unwrap();
        })
        .unwrap()
    }

    #[test]
    fn typed_worker_log_append_async_callback_runs_after_log_is_persisted() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = temp_dir().join(format!(
                "typed_worker_log_append_async_callback_{}",
                gen_oid()
            ));
            let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
            let path = layout.chunk_path(0);
            let backend = ChunkedWorkerLogBackend::new(layout).await.unwrap();
            let log = TypedWorkerLog::new(backend, NoopHandler);
            let entry = TestEntry {
                id: 8,
                payload: vec![8; 32],
            };

            let expected = entry.clone();
            let callback_path = path.clone();
            log.append_async_callback(entry, move |written| {
                let path = callback_path.clone();
                let expected = expected.clone();
                let written = written.clone();
                async move {
                    assert_eq!(written, expected);
                    let bytes = mudu_sys::fs::async_::read_all(&path).await?;
                    assert!(!bytes.is_empty());
                    Ok(())
                }
            })
            .await
            .unwrap();
        })
        .unwrap()
    }
}
