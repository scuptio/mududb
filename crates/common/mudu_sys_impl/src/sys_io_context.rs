use crate::common::provider_type::ProviderType;
use crate::contract::async_fs::AsyncFs;
use crate::contract::async_io_provider::AsyncIoProvider;
use crate::imp::fs::sync::FsSync;
use crate::provider::create_io_provider;
use std::sync::{Arc, OnceLock};

/// System IO context bundling the selected async IO provider.
///
/// The context hands out the async filesystem (and, through the provider,
/// the async networking backend) together with a synchronous filesystem
/// facade, so callers do not have to thread the provider through themselves.
#[derive(Clone)]
pub struct SysIoContext {
    io: Arc<dyn AsyncIoProvider>,
}

impl SysIoContext {
    /// Wraps an already constructed async IO provider in a new context.
    pub fn new(io: Arc<dyn AsyncIoProvider>) -> Arc<Self> {
        Arc::new(Self { io })
    }

    /// Creates a context backed by a fresh provider of the given type.
    pub fn with_provider(provider_type: ProviderType) -> Arc<Self> {
        Self::new(create_io_provider(provider_type))
    }

    /// Creates a context backed by the Tokio async IO provider.
    pub fn tokio() -> Arc<Self> {
        Self::with_provider(ProviderType::Tokio)
    }

    /// Creates a context backed by the Linux io_uring async IO provider.
    #[cfg(target_os = "linux")]
    pub fn iouring() -> Arc<Self> {
        Self::with_provider(ProviderType::IoUring)
    }

    /// Returns the underlying async IO provider as a trait object reference.
    pub fn provider(&self) -> &dyn AsyncIoProvider {
        self.io.as_ref()
    }

    /// Returns a shared handle to the underlying async IO provider.
    pub fn provider_arc(&self) -> Arc<dyn AsyncIoProvider> {
        self.io.clone()
    }

    /// Returns the async filesystem interface of the underlying provider.
    pub fn fs(&self) -> Arc<dyn AsyncFs> {
        self.io.fs_arc()
    }

    /// Returns a synchronous filesystem facade.
    pub fn fs_sync(&self) -> FsSync {
        FsSync::new()
    }
}

/// Returns the process-wide default IO context.
///
/// The default context uses the Tokio provider and is created lazily on
/// first use.
pub fn default_sys_io_context() -> Arc<SysIoContext> {
    static DEFAULT: OnceLock<Arc<SysIoContext>> = OnceLock::new();
    DEFAULT.get_or_init(SysIoContext::tokio).clone()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::contract::async_mode::AsyncMode;
    use crate::contract::file_options::FileOptions;
    use crate::imp::fs::SysFile;

    #[tokio::test]
    async fn cloned_contexts_and_files_share_the_same_backend() {
        let first = default_sys_io_context();
        let second = default_sys_io_context();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.provider().mode(), AsyncMode::Tokio);
        assert!(Arc::ptr_eq(&first.provider_arc(), &second.provider_arc()));
        let independent = SysIoContext::new(first.provider_arc());
        assert!(Arc::ptr_eq(&first.fs(), &independent.fs()));
        let root = std::env::temp_dir().join(format!("sys-context-{}", crate::random::uuid_v4()));
        first.fs_sync().create_dir_all(&root).unwrap();
        let path = root.join("shared");
        let file = SysFile::new(
            first
                .fs()
                .open(&path, FileOptions::read_write_create())
                .await
                .unwrap(),
        );
        let clone = file.clone();
        file.write_all_at(0, b"shared").await.unwrap();
        clone.fsync().await.unwrap();
        assert_eq!(clone.file_len().await.unwrap(), 6);
        assert_eq!(clone.read_exact_at(0, 6).await.unwrap(), b"shared");
        #[cfg(unix)]
        assert!(clone.as_raw_fd().is_some());
        clone.close().await.unwrap();
        drop(clone);
        drop(file);
        first.fs_sync().remove_dir_all(&root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn iouring_context_selects_the_requested_provider() {
        let context = SysIoContext::iouring();
        assert_eq!(context.provider().mode(), AsyncMode::IoUring);
        assert!(Arc::ptr_eq(&context.fs(), &context.provider_arc().fs_arc()));
    }
}
