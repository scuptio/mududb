use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::fs::SysFile;
use mudu_sys::scoped_task_trace;
use std::path::Path;

use super::backend::WorkerWALBackend;

impl WorkerWALBackend {
    pub(crate) async fn take_or_open_async_file(&self, path: &Path) -> RS<SysFile> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.file.stage", "take_cached_file");
        if let Some(file) = self.take_cached_file(path)? {
            trace.watch("wal.file.stage", "cache_hit");
            return Ok(file);
        }
        trace.watch("wal.file.stage", "open_async");
        self.open_async(path).await
    }

    pub(crate) fn take_or_open_sync_file(&self, path: &Path) -> RS<SysFile> {
        if let Some(file) = self.take_cached_file(path)? {
            return Ok(file);
        }
        self.open_sync(path)
    }

    fn take_cached_file(&self, path: &Path) -> RS<Option<SysFile>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker kv log lock poisoned"))?;
        Ok(guard.take_current_file(path))
    }

    pub(crate) async fn release_async_file(&self, path: &Path, file: SysFile) -> RS<()> {
        if let Some(file) = self.put_cached_file(path, file)? {
            file.close().await?;
        }
        Ok(())
    }

    pub(crate) async fn flush_path_async(&self, path: &Path) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let file = self.take_or_open_async_file(path).await?;
        let flush_result = file.fsync().await;
        self.finish_async_file_use(path, file, flush_result).await?;
        Ok(())
    }

    async fn finish_async_file_use(&self, path: &Path, file: SysFile, result: RS<()>) -> RS<()> {
        scoped_task_trace!();
        self.finish_async_file_use_with_value(path, file, result)
            .await?;
        Ok(())
    }

    async fn finish_async_file_use_with_value<T>(
        &self,
        path: &Path,
        file: SysFile,
        result: RS<T>,
    ) -> RS<T> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.file.stage", "release_async_file");
        let close_result = self.release_async_file(path, file).await;
        trace.watch("wal.file.stage", "awaited_result");
        let value = result?;
        trace.watch("wal.file.stage", "awaited_close");
        close_result?;
        trace.watch("wal.file.stage", "finish_done");
        Ok(value)
    }

    pub(crate) fn release_sync_file(&self, path: &Path, file: SysFile) -> RS<()> {
        if let Some(file) = self.put_cached_file(path, file)? {
            Self::close_sync(file)?;
        }
        Ok(())
    }

    fn put_cached_file(&self, path: &Path, file: SysFile) -> RS<Option<SysFile>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker kv log lock poisoned"))?;
        Ok(guard.store_current_file(path, file))
    }

    async fn open_async(&self, path: &Path) -> RS<SysFile> {
        scoped_task_trace!();
        let file = self
            .inner
            .io
            .fs()
            .open(
                path,
                FileOptions::new(libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC, 0o644),
            )
            .await?;
        Ok(SysFile::new(file))
    }

    fn open_sync(&self, path: &Path) -> RS<SysFile> {
        futures::executor::block_on(self.open_async(path))
    }

    pub(crate) fn flush_sync(file: &SysFile) -> RS<()> {
        futures::executor::block_on(file.fsync())
    }

    fn close_sync(file: SysFile) -> RS<()> {
        {
            drop(file);
            Ok(())
        }
    }
}
