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
